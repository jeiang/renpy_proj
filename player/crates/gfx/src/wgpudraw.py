# The Python half of the wgpu renderer. The Rust side of module renpy.gl2.wgpudraw runs this file into the module
# namespace, so `Gpu`, `GpuTexture`, `GpuProgram`, `compile_program`, `create`, `create_headless` are already defined.
#
# WgpuDraw is a drop-in for renpy.gl2.gl2draw.GL2Draw (Ren'Py 8.5.3, MIT licence, Copyright 2004-2026 Tom Rothamel):
# same members, same draw semantics. The Render tree walk and the uniform getters are in renpy.gl2.gl2meshbridge
# (Cython, needs cdef access). Shader translation, pipelines, buffers and passes are Rust.

import math
import os
import time
import weakref

import renpy
import renpy.pygame as pygame
from renpy.pygame import Surface

from renpy.display.matrix import Matrix, Matrix2D, IDENTITY
from renpy.gl2.gl2model import GL2Model
from renpy.gl2.gl2mesh2 import Mesh2
from renpy.gl2.gl2mesh3 import Mesh3
from renpy.gl2.gl2shadercache import ShaderCache
import renpy.gl2.gl2meshbridge as _bridge

GL_NEAREST = 0x2600
GL_LINEAR = 0x2601
GL_LINEAR_MIPMAP_NEAREST = 0x2701

# Should we try to vsync?
vsync = True

# A list of frame end times, used for the same purpose.
frame_times = []

# The Gpu of the running WgpuDraw, so that programs compiled during init can warm their pipelines.
gpu = None


def warm_program(handle):
    """
    Starts building the common pipelines of a freshly compiled program.
    """

    if gpu is not None:
        handle.warm(gpu)


class TextureLoader(object):
    """
    Loads surfaces and rendered trees into textures, tiling images that are bigger than the maximum texture size.
    """

    def __init__(self, draw):
        self.draw = draw
        self.texture_load_queue = weakref.WeakSet()
        self.max_texture_width = 4096
        self.max_texture_height = 4096

    def init(self):
        self.texture_load_queue = weakref.WeakSet()

    def quit(self):
        self.texture_load_queue = weakref.WeakSet()

    def get_texture_size(self):
        """
        Returns the amount of memory locked up in textures, and the number of textures.
        """

        return self.draw.gpu.texture_size()

    def load_one_surface(self, surf, bl, bt, br, bb, properties):
        """
        Converts a surface into a texture.
        """

        size = surf.get_size()

        rv = Texture(size, self)
        rv.from_surface(surf, properties)

        if bl or bt or br or bb:

            rv.bl = bl
            rv.bt = bt
            rv.br = br
            rv.bb = bb

            w, h = size

            pw = w - bl - br
            ph = h - bt - bb

            if (w and h):

                mesh = Mesh2.texture_rectangle(
                    0.0, 0.0, pw, ph,
                    1.0 * bl / w, 1.0 * bt / h, 1.0 - 1.0 * br / w, 1.0 - 1.0 * bb / h)
            else:
                mesh = Mesh2.texture_rectangle(
                    0.0, 0.0, pw, ph,
                    0.0, 0.0, 0.0, 0.0)

            old_rv = rv
            rv = GL2Model((pw, ph), mesh, ("renpy.texture",))
            rv.set_texture(0, old_rv)

        return rv

    def texture_axis(self, length, limit, border):
        """
        Splits `length` up into multiple textures.

        This returns a series of (offset, width, left/top border, right/bottom border) tuples.
        """

        if length <= limit:
            return [(0, length, 0, 0)]

        elif length <= 2 * (limit - border):

            right = length // 2
            left = length - right

            return [(0, left, 0, border), (left, right, border, 0)]

        else:
            tiles = math.ceil(1.0 * length / (limit - 2 * border))
            tile_length = length / tiles
            tiles = int(tiles)

            rv = []

            for i in range(tiles):
                start = int(i * tile_length)
                end = int((i + 1) * tile_length)

                if i > 0:
                    left = border
                else:
                    left = 0

                if i < tiles - 1:
                    right = border
                else:
                    right = 0

                rv.append((start, end - start, left, right))

        return rv

    def load_surface(self, surf, properties):
        border = 1

        size = surf.get_size()
        w, h = size

        if (w <= self.max_texture_width) and (h <= self.max_texture_height):
            return self.load_one_surface(surf, 0, 0, 0, 0, properties)

        htiles = self.texture_axis(w, self.max_texture_width, border)
        vtiles = self.texture_axis(h, self.max_texture_height, border)

        rv = renpy.display.render.Render(w, h)

        for ty, th, bt, bb in vtiles:
            for tx, tw, bl, br in htiles:
                ss = surf.subsurface((tx - bl, ty - bt, tw + bl + br, th + bt + bb))
                t = self.load_one_surface(ss, bl, bt, br, bb, properties)
                rv.blit(t, (tx, ty))

        return rv

    def render_to_texture(self, what, properties, oversample=1.0):
        """
        Renders `what` to a texture.
        """

        rv = Texture(what.get_size(), self)
        rv.from_render(what, properties, oversample)
        return rv

    def cleanup(self):
        """
        Called once per frame. Textures free their GPU memory when Python drops them, so there is nothing to do.
        """

    def ready_one_texture(self):
        """
        Called by WgpuDraw to implement ready_one_texture.
        """

        while True:

            try:
                tex = self.texture_load_queue.pop()
            except KeyError:
                return False

            if not tex.loaded:
                tex.load()
                return True

        return False


class Texture(GL2Model):
    """
    A texture that lives on the GPU. It is a GL2Model leaf that draws itself with the renpy.texture shader. Loading
    from a surface is deferred until the texture is first needed; the Rust texture is freed with this object.
    """

    def __init__(self, size, loader, handle=None):
        width, height = size

        GL2Model.__init__(self, size, None, ("renpy.texture",), None)

        # The Rust GpuTexture, or None until loaded.
        self.handle = handle

        # True if the texture has been uploaded.
        self.loaded = handle is not None

        # Used for loading surfaces.
        self.surface = None

        self.loader = loader

        # Borders.
        self.bl = 0
        self.bt = 0
        self.br = 0
        self.bb = 0

        # The size of the GPU texture (which render_to_texture can make different from the visible size).
        self.texture_width = width
        self.texture_height = height

        # Used by the sampler state.
        self.default_mag_filter = GL_LINEAR
        self.default_min_filter = GL_LINEAR

    def __repr__(self):
        return "<Texture {}x{} {}>".format(self.width, self.height, "loaded" if self.loaded else "pending")

    def should_have_mipmaps(self, properties):
        """
        Returns true if this texture has mipmaps (or will have mipmaps when it's loaded).
        """

        rv = properties.get("mipmap", renpy.config.mipmap)

        if rv == "auto":
            rv = renpy.display.draw.auto_mipmap

        return rv

    def has_mipmaps(self):
        return self.properties["mipmap"]

    def get_number(self):
        return None

    def from_surface(self, surface, properties):
        """
        Called to indicate this texture should be loaded from a surface.
        """

        self.surface = surface
        self.properties = dict(properties)
        self.properties["mipmap"] = self.should_have_mipmaps(properties)

        self.mesh = Mesh2.texture_rectangle(
            0.0, 0.0, self.width, self.height,
            0.0, 0.0, 1.0, 1.0,
            )

        self.loader.texture_load_queue.add(self)

    def from_render(self, what, properties, oversample=1.0):
        """
        This renders `what` to this texture.
        """

        self.properties = {
            "mipmap": self.should_have_mipmaps(properties),
            "pixel_perfect": properties.get("pixel_perfect", False),
            }

        cw, ch = what.get_size()

        loader = self.loader
        draw = loader.draw
        gpu = draw.gpu

        # The visible size of the texture.

        drawable = properties.get("drawable_resolution", True)

        if drawable:

            tw, th = draw.virt_to_draw.transform(cw * oversample, ch * oversample)

            tw = round(tw)
            th = round(th)

        else:

            tw = cw = round(cw * oversample)
            th = ch = round(ch * oversample)

        tw = min(tw, loader.max_texture_width)
        th = min(th, loader.max_texture_height)

        tw = max(tw, 1)
        th = max(th, 1)

        cw = max(cw, 1)
        ch = max(ch, 1)

        self.mesh = Mesh2.texture_rectangle(
            0.0, 0.0, cw, ch,
            0.0, 0.0, 1.0, 1.0,
            )

        mipmap = bool(self.properties["mipmap"])
        handle = gpu.new_texture(tw, th, mipmap)

        # Project the child from virtual space to the texture space.
        transform = Matrix.texture_projection(cw, ch)

        gpu.begin_pass(handle, (0, 0, tw, th), (0.0, 0.0, 0.0, 0.0))

        try:
            _bridge.draw_render(gpu, draw, what, tw, th, transform)
        finally:
            gpu.end_pass()

        if mipmap:
            gpu.queue_mips(handle)

        self.handle = handle
        self.texture_width = tw
        self.texture_height = th
        self.default_min_filter = GL_LINEAR_MIPMAP_NEAREST if mipmap else GL_LINEAR
        self.loaded = True

    def load(self):
        """
        Uploads this texture, if it has not been uploaded.
        """

        if self.loaded:
            return

        mipmap = bool(self.properties["mipmap"])

        if self.width <= 0 or self.height <= 0 or min(self.surface.get_size()) <= 0:

            # A zero-size surface becomes an empty texture, as in stock GL2. The mesh has no area, so nothing is
            # drawn; wgpu cannot allocate a texture with a zero side, so the GPU texture is 1x1 and transparent.
            mipmap = False
            self.properties["mipmap"] = False
            self.handle = self.loader.draw.gpu.new_texture(1, 1, False)

            self.texture_width = 1
            self.texture_height = 1
            self.default_min_filter = GL_LINEAR
            self.loaded = True
            self.surface = None
            return

        self.handle = self.loader.draw.gpu.texture_from_surface(
            self.surface,
            bool(self.properties.get("premultiplied", False)),
            mipmap)

        self.texture_width = self.width
        self.texture_height = self.height
        self.default_min_filter = GL_LINEAR_MIPMAP_NEAREST if mipmap else GL_LINEAR

        self.loaded = True
        self.surface = None

    def add_mipmap(self):
        """
        Adds a mipmap to this texture, if it doesn't exist.
        """

        if self.properties["mipmap"]:
            return

        self.properties["mipmap"] = True

        if self.loaded:
            self.loader.draw.gpu.add_mipmap(self.handle)
            self.default_min_filter = GL_LINEAR_MIPMAP_NEAREST

    def subsurface(self, rect):
        rv = GL2Model.subsurface(self, rect)
        if rv is not self:
            rv.set_texture(0, self)
        return rv

    def get_texture(self, i):
        """
        Returns the texture at index `i`.
        """

        if i == 0:
            return self
        else:
            raise IndexError("Texture.get_texture: index out of range")


class WgpuDraw(object):
    """
    The renpy.display.draw object for the wgpu renderer.
    """

    def __init__(self, name="wgpu"):

        # The Rust Gpu, once init has run.
        self.gpu = None

        # The screen.
        self.window = None

        # The virtual size of the screen, as requested by the game.
        self.virtual_size = None

        # The physical size of the window we got.
        self.physical_size = None

        # The time of the last redraw.
        self.last_redraw_time = 0

        # The time between redraws.
        self.redraw_period = .2

        # Info.
        self.info = {"resizable": True, "additive": True, "renderer": "wgpu", "models": True}

        # We don't use a fullscreen surface, so this needs to be set
        # to None at all times.
        self.fullscreen_surface = None

        # The display info, from pygame.
        self.display_info = None

        # The DPI scale factor.
        self.dpi_scale = renpy.display.interface.dpi_scale

        # The number of frames to draw fast if the screen needs to be
        # updated.
        self.fast_redraw_frames = 0

        # The shader cache,
        self.shader_cache = None
        self.first_frame_logged = False
        self.first_frame_times = []
        self.frame_count = 0
        self.slow_frames_logged = 0
        self.skipped_seen = 0
        self.last_flip_end = 0.0
        self.refresh_rate = 60

        # The texture loader.
        self.texture_loader = None

        # Has the position of this window ever been set?
        self.ever_set_position = False

        # Was the window maximized the last time update was called?
        self.maximized = False

        # The old value of fullscreen.
        self.old_fullscreen = False

        # Should mipmaps be generated when mipmap == "auto"?
        self.auto_mipmap = False

        # A 1x1 target for is_pixel_opaque.
        self.pixel_target = None

    def get_texture_size(self):
        """
        Returns the amount of memory locked up in textures.
        """

        if self.texture_loader is None:
            return 0, 0

        return self.texture_loader.get_texture_size()

    def select_physical_size(self, physical_size):
        """
        *Internal* Determines the 'best' physical size to use, and returns
        it.
        """

        limit_physical_size = True

        if physical_size and renpy.game.preferences.window_position_layout == renpy.game.interface.get_display_layout():
            limit_physical_size = False

        # Are we maximized?
        old_surface = pygame.display.get_surface()
        if old_surface is not None:
            flags = old_surface.get_flags()
            maximized = (flags & pygame.WINDOW_MAXIMIZED) and not (flags & (pygame.WINDOW_FULLSCREEN | pygame.WINDOW_FULLSCREEN_DESKTOP))
        else:
            maximized = renpy.game.preferences.maximized

        # Information about the virtual size.
        vwidth, vheight = self.virtual_size
        virtual_ar = 1.0 * vwidth / vheight

        # The requested size.
        pwidth, pheight = physical_size

        if pwidth is None:
            pwidth = vwidth
            pheight = vheight

        # If a DPI scale is present, take it into account.
        pwidth *= self.dpi_scale
        pheight *= self.dpi_scale

        # Determine the visible area of the screen.
        info = renpy.display.get_info()

        visible_w = info.current_w
        visible_h = info.current_h

        # Determine the visible area of the current head.
        bounds = pygame.display.get_display_bounds(0)

        renpy.display.log.write("primary display bounds: %r", bounds)

        head_w = bounds[2] - 102
        head_h = bounds[3] - 102

        # Figure out the default window size.
        bound_w = min(visible_w, head_w)
        bound_h = min(visible_h, head_h)

        self.info["max_window_size"] = (
            round(min(bound_h * virtual_ar, bound_w)),
            round(min(bound_w / virtual_ar, bound_h)),
            )

        if (not renpy.mobile) and (not maximized):

            if limit_physical_size:

                # Limit to the visible area
                pwidth = min(visible_w, pwidth)
                pheight = min(visible_h, pheight)

                pwidth = min(pwidth, head_w)
                pheight = min(pheight, head_h)

            # Has to be a one-liner as the two values depend on each other.
            pwidth, pheight = min(pheight * virtual_ar, pwidth), min(pwidth / virtual_ar, pheight)

        # Limit to integers.
        pwidth = round(pwidth)
        pheight = round(pheight)

        # Keep a minimum size.
        pwidth = max(pwidth, 256)
        pheight = max(pheight, 256)

        return pwidth, pheight

    def select_framerate(self):
        """
        *Internal*
        This selects the framerate to use and the interval between frames.
        """

        global vsync

        info = renpy.display.get_info()

        target_framerate = renpy.game.preferences.gl_framerate
        refresh_rate = info.refresh_rate

        if not refresh_rate:
            refresh_rate = 60

        if target_framerate is None:
            sync_frames = 1
        else:
            sync_frames = int(round(1.0 * refresh_rate) / target_framerate)
            if sync_frames < 1:
                sync_frames = 1

        if renpy.game.preferences.gl_tearing:
            sync_frames = -sync_frames

        vsync = int(os.environ.get("RENPY_GL_VSYNC", sync_frames))

        if not renpy.config.gl_vsync:
            vsync = 0

        renpy.display.interface.frame_duration = 1.0 * abs(vsync or 1) / refresh_rate
        self.refresh_rate = refresh_rate

        if self.gpu is not None:
            self.gpu.set_vsync(bool(vsync) and vsync > 0)

        renpy.display.log.write("swap interval: %r frames", vsync)

    def get_window_position(self, physical_size=None):
        """
        Determines the position of the window, based on what's stored
        in preferences.

        The stored position is used only when the total display size hasn't
        changed, and the window would not overlap the edge of the screen.
        """

        default = (pygame.WINDOWPOS_CENTERED, pygame.WINDOWPOS_CENTERED)

        if "RENPY_CENTER_WINDOW" in os.environ:
            return default

        if renpy.display.interface.safe_mode:
            return default

        if not (renpy.linux or renpy.windows or renpy.macintosh):
            return default

        if not renpy.game.preferences.restore_window_position:
            return default

        layout = renpy.game.interface.get_display_layout()

        if renpy.game.preferences.window_position_layout != layout:
            return default

        pos = renpy.game.preferences.window_position

        for rect in layout:

            if pos[0] < rect[0] or pos[1] < rect[1]:
                continue

            if physical_size is not None:
                pwidth, pheight = physical_size
                if pos[0] + pwidth > rect[2] and pos[1] + pheight > rect[3]:
                    continue

            return pos

        return default

    def init(self, virtual_size):
        """
        This changes the video mode and creates the GPU device. It returns True if it was successful, or False if
        the device could not be created.
        """

        global gpu

        self.virtual_size = virtual_size

        if not renpy.config.gl_enable:
            renpy.display.log.write("GL Disabled.")
            return False

        if renpy.mobile:
            physical_size = (None, None)
        elif renpy.game.preferences.physical_size is None:
            physical_size = (renpy.config.physical_width, renpy.config.physical_height)
        else:
            physical_size = renpy.game.preferences.physical_size

        pwidth, pheight = self.select_physical_size(physical_size)

        if renpy.android or renpy.ios:
            fullscreen = True
        else:
            fullscreen = renpy.game.preferences.fullscreen

        self.select_framerate()

        window_flags = 0

        if self.dpi_scale == 1.0:
            window_flags |= pygame.WINDOW_ALLOW_HIGHDPI

        if renpy.config.gl_resize:
            window_flags |= pygame.RESIZABLE

        if renpy.config.gl2_modify_window_flags is not None:
            window_flags = renpy.config.gl2_modify_window_flags(window_flags)

        # Opens the window.
        #
        # If we're in fullscreen, tries to get a fullscreen window. If that fails,
        # or fullscreen is False, tries to open a normal window.

        self.window = None

        if fullscreen:
            try:
                renpy.display.log.write("Fullscreen mode.")
                self.window = pygame.display.set_mode((0, 0), pygame.WINDOW_FULLSCREEN_DESKTOP | window_flags)
            except pygame.error as e:
                renpy.display.log.write("Opening in fullscreen failed: %r", e)
                self.window = None

        if self.window is None:

            if renpy.game.preferences.maximized:
                window_flags |= pygame.WINDOW_MAXIMIZED
                pos = (pygame.WINDOWPOS_UNDEFINED, pygame.WINDOWPOS_UNDEFINED)
            else:
                self.ever_set_position = True
                pos = self.get_window_position((pwidth, pheight))

            try:
                renpy.display.log.write("Windowed mode.")
                self.window = pygame.display.set_mode((pwidth, pheight), window_flags, pos=pos)
            except pygame.error as e:
                renpy.display.log.write("Could not get pygame screen: %r", e)
                return False

        if "RENPY_FAIL_" + self.info["renderer"].upper() in os.environ:
            self.quit()
            return False

        # Create the wgpu device on the window.
        try:
            new_gpu = create(pwidth, pheight, renpy.config.window_title or "Ren'Py", bool(renpy.config.gl_resize))
        except Exception as e:
            renpy.display.log.write("Could not create the wgpu device: %s", e)
            return False

        self.gpu = gpu = new_gpu
        self.gpu.set_vsync(bool(vsync) and vsync > 0)

        gpu_info = self.gpu.info()
        self.info["gpu_name"] = gpu_info["gpu_name"]
        self.info["gpu_vendor"] = gpu_info["gpu_backend"]
        self.info["gpu_driver_version"] = "wgpu " + gpu_info["gpu_backend"]
        self.info["max_texture_size"] = gpu_info["max_texture_size"]

        renpy.display.log.write(f"Renderer: {gpu_info['gpu_name']!r}")
        renpy.display.log.write(f"Backend: {gpu_info['gpu_backend']!r}")

        self.display_info = renpy.display.get_info()
        renpy.display.log.write(f"Display Info: {self.display_info}")

        # Do additional setup needed.
        renpy.display.pgrender.set_rgba_masks()

        if renpy.android or renpy.ios:
            self.redraw_period = 1.0

        self.shader_cache = ShaderCache("cache/shaders.txt", False)

        # Initialize the texture loader.
        self.texture_loader = TextureLoader(self)

        self.on_resize(first=True)

        return True

    def precompile_shaders(self):
        """
        Translates every built-in shader program a game draws with, so that their pipelines build on worker threads
        before the first frame needs them. The programs are each `renpy.*` part alone and each part together with
        `renpy.texture`. Programs listed in `cache/shaders.txt` were already compiled by `ShaderCache.load`.
        """

        from renpy.gl2.gl2shadercache import shader_part

        t0 = time.time()

        names = sorted(n for n in shader_part if n.startswith("renpy.") and n != renpy.config.default_shader)
        combos = [(n,) for n in names]
        combos += [("renpy.texture", n) for n in names if n != "renpy.texture"]

        for partnames in combos:
            if partnames in self.shader_cache.cache:
                continue
            try:
                self.shader_cache.get(partnames)
            except Exception:
                renpy.display.log.write("Precompiling shader %r failed:", partnames)
                renpy.display.log.exception()

        renpy.display.log.write("Shaders precompiled at load: %d programs in %.1f ms; pipelines build in the background.", len(self.shader_cache.cache), (time.time() - t0) * 1000.0)

    def on_resize(self, first=False, full_reset=False):

        if first:
            full_reset = True

        if renpy.android or renpy.ios:
            full_reset = True

        if not first and full_reset:
            self.shader_cache.clear()

        # Are we in fullscreen mode?
        fullscreen = bool(pygame.display.get_window().get_window_flags() & (pygame.WINDOW_FULLSCREEN_DESKTOP | pygame.WINDOW_FULLSCREEN))

        # Are we maximized?
        maximized = bool(pygame.display.get_window().get_window_flags() & pygame.WINDOW_MAXIMIZED) and not fullscreen and renpy.config.gl_resize

        # See if we've ever set the screen position, and if not, center the window.
        if not fullscreen and not maximized and not self.ever_set_position:
            self.ever_set_position = True
            pygame.display.get_window().set_position(self.get_window_position())

        # Get the size of the created screen.
        pwidth, pheight = renpy.display.core.get_size()

        vwidth, vheight = self.virtual_size

        self.physical_size = (pwidth, pheight)
        self.drawable_size = pygame.display.get_drawable_size()

        renpy.display.log.write("Screen sizes: virtual=%r physical=%r drawable=%r" % (self.virtual_size, self.physical_size, self.drawable_size))

        # The swap chain follows the drawable size.
        self.gpu.resize(*self.drawable_size)

        # Update the preferences.
        renpy.game.preferences.fullscreen = fullscreen
        renpy.game.interface.fullscreen = fullscreen

        if not fullscreen:
            renpy.game.preferences.maximized = maximized

        if not fullscreen and not maximized:
            renpy.game.preferences.physical_size = self.get_physical_size()

        if renpy.config.adjust_view_size is not None:
            view_width, view_height = renpy.config.adjust_view_size(pwidth, pheight)
        else:

            # Figure out the virtual box, which includes padding around
            # the borders.
            ratio = min(1.0 * pwidth / vwidth, 1.0 * pheight / vheight)

            view_width = max(int(vwidth * ratio), 1)
            view_height = max(int(vheight * ratio), 1)

        px_padding = pwidth - view_width
        py_padding = pheight - view_height

        # Ren'Py 7 divides these integers with Python 2 `/` (floor). Ren'Py 8 divides them true, which puts the picture
        # half a physical pixel to the right or down when the padding is odd (`px_padding / 2` is 0.5, not 0).
        try:
            import _player.compat
            floor_div = _player.compat.active
        except ImportError:
            floor_div = False

        if floor_div:
            x_padding = px_padding * vwidth // view_width
            y_padding = py_padding * vheight // view_height
            px_half = px_padding // 2
            py_half = py_padding // 2
        else:
            x_padding = px_padding * vwidth / view_width
            y_padding = py_padding * vheight / view_height
            px_half = px_padding / 2
            py_half = py_padding / 2

        # The position of the physical screen, in virtual pixels
        # (x, y, w, h). Since the physical screen will always contain
        # the virtual screen, the corners are often off the virtual
        # screen.
        self.virtual_box = (
            -x_padding / 2.0,
            -y_padding / 2.0,
             vwidth + x_padding,
             vheight + y_padding)

        # The location of the virtual screen on the physical screen, in
        # physical pixels.
        self.physical_box = (
            px_half,
            py_half,
            pwidth - px_padding,
            pheight - py_padding,
            )

        # The scaling factor of physical_pixels to drawable pixels.
        self.draw_per_phys = 1.0 * self.drawable_size[0] / self.physical_size[0]

        # The location of the viewport, in drawable pixels.
        self.drawable_viewport = tuple(i * self.draw_per_phys for i in self.physical_box)

        dwidth = self.drawable_viewport[2]
        dheight = self.drawable_viewport[3]

        # How many drawable pixels there are per virtual pixel?
        self.draw_per_virt = 1.0 * self.drawable_viewport[2] / vwidth

        # Matrices that transform from virtual space to drawable space, and vice versa.
        self.virt_to_draw = Matrix2D(1.0 * dwidth / vwidth, 0, 0, 1.0 * dheight / vheight)
        self.draw_to_virt = Matrix2D(1.0 * vwidth / dwidth, 0, 0, 1.0 * vheight / dheight)

        self.draw_transform = Matrix.screen_projection(self.drawable_viewport[2], self.drawable_viewport[3])

        self.init_limits()

        if full_reset:
            self.shader_cache.load()
            self.precompile_shaders()
            self.texture_loader.init()
        else:
            self.texture_loader.cleanup()

        self.auto_mipmap = self.draw_per_virt < 0.75

    def init_limits(self):
        """
        *Internal*
        Determines the largest texture Ren'Py will make.
        """

        # The number of pixels of additional border, as in stock Ren'Py.
        BORDER = 64

        max_texture_size = max(self.info["max_texture_size"], 1024)

        width, height = renpy.config.max_texture_size

        width = max(self.virtual_size[0] + BORDER, self.drawable_size[0] + BORDER, width)
        width = min(width, max_texture_size)
        height = max(self.virtual_size[1] + BORDER, self.drawable_size[1] + BORDER, height)
        height = min(height, max_texture_size)

        if "RENPY_MAX_TEXTURE_SIZE" in os.environ:
            width = height = int(os.environ["RENPY_MAX_TEXTURE_SIZE"])

        renpy.display.log.write("Maximum texture size: %dx%d", width, height)

        self.texture_loader.max_texture_width = width
        self.texture_loader.max_texture_height = height

    def resize(self):
        """
        Documented in renderer.
        """

        fullscreen = renpy.game.preferences.fullscreen

        if renpy.android or renpy.ios:
            fullscreen = True

        if renpy.game.preferences.physical_size:
            width = renpy.game.preferences.physical_size[0] or self.virtual_size[0]
            height = renpy.game.preferences.physical_size[1] or self.virtual_size[1]
        else:
            width = self.virtual_size[0]
            height = self.virtual_size[1]

        width *= self.dpi_scale
        height *= self.dpi_scale

        if not renpy.android or renpy.ios:
            max_w, max_h = self.info["max_window_size"]
            width = min(width, max_w)
            height = min(height, max_h)

        width = max(width, 256)
        height = max(height, 256)

        if fullscreen:
            maximized = False
        else:
            maximized = renpy.game.preferences.maximized

        renpy.display.log.write("Requested resize to %dx%d, fullscreen=%d, maximized=%d", width, height, fullscreen, maximized)
        pygame.display.get_window().resize((width, height), opengl=False, fullscreen=fullscreen, maximized=maximized)

        renpy.display.interface.fullscreen = fullscreen

    def update(self, force=False):
        """
        Documented in renderer.
        """

        flags = pygame.display.get_window().get_window_flags()

        fullscreen = bool(flags & (pygame.WINDOW_FULLSCREEN_DESKTOP | pygame.WINDOW_FULLSCREEN))

        maximized = bool(flags & pygame.WINDOW_MAXIMIZED)

        size = renpy.display.core.get_size()
        drawable_size = pygame.display.get_drawable_size()

        if (
            (force) or
            (fullscreen != renpy.display.interface.fullscreen) or
            (size != self.physical_size) or
            (drawable_size != self.drawable_size) or
            (self.maximized != maximized)
        ):

            self.maximized = maximized
            full_reset = renpy.display.interface.display_reset
            renpy.display.interface.before_resize()
            self.on_resize(full_reset=full_reset)

            return True
        else:
            return False

    def quit(self):
        """
        Called when terminating the use of the renderer.
        """

        global gpu

        self.kill_textures()

        if self.texture_loader is not None:
            self.texture_loader.quit()
            self.texture_loader = None

        self.pixel_target = None

        if self.shader_cache is not None:
            self.shader_cache.save()

        if gpu is self.gpu:
            gpu = None

        self.gpu = None

    def can_block(self):
        """
        Returns True if we can block to wait for input, False if the screen
        needs to be immediately redrawn.
        """

        powersave = renpy.game.preferences.gl_powersave

        if not powersave:
            return False

        return not self.fast_redraw_frames

    def should_redraw(self, needs_redraw, first_pass, can_block):
        """
        Redraw whenever the screen needs it, but at least once every
        .2 seconds. We rely on VSYNC to slow down our maximum
        draw speed.
        """

        rv = False

        if needs_redraw:
            rv = True
        elif first_pass:
            rv = True

        # Handle fast redraw.
        if rv:
            self.fast_redraw_frames = renpy.config.fast_redraw_frames
        elif self.fast_redraw_frames > 0:
            self.fast_redraw_frames -= 1
            rv = True

        if time.time() > self.last_redraw_time + self.redraw_period:
            rv = True

        # Store the redraw time.
        if rv or (not can_block):
            self.last_redraw_time = time.time()
            return True
        else:
            return False

    def mutated_surface(self, surf):
        return

    def load_texture(self, surf, transient=False, properties={}):
        """
        Loads a texture into memory.
        """

        return self.texture_loader.load_surface(surf, properties)

    def load_video_frame(self, frame, mipmap=False):
        """
        Turns a media.VideoFrame into a texture: the planes are uploaded and converted to RGBA on the GPU.
        """

        tex = Texture((frame.width, frame.height), self.texture_loader)

        tex.properties = {"mipmap": tex.should_have_mipmaps({"mipmap": mipmap})}
        mipmap = bool(tex.properties["mipmap"])

        tex.handle = self.gpu.load_video_frame(frame, mipmap)
        tex.loaded = True
        tex.default_min_filter = GL_LINEAR_MIPMAP_NEAREST if mipmap else GL_LINEAR

        tex.mesh = Mesh2.texture_rectangle(
            0.0, 0.0, frame.width, frame.height,
            0.0, 0.0, 1.0, 1.0,
            )

        return tex

    def ready_one_texture(self):
        """
        Call from the main thread to make a single texture ready.
        """

        if self.texture_loader is None:
            return False

        return self.texture_loader.ready_one_texture()

    def solid_texture(self, w, h, color):
        """
        Returns a texture that represents a solid color.
        """

        mesh = Mesh3.rectangle(0, 0, w, h)

        a = color[3] / 255.0
        r = a * color[0] / 255.0
        g = a * color[1] / 255.0
        b = a * color[2] / 255.0

        color = (r, g, b, a)

        return GL2Model((w, h), mesh, ("renpy.solid", ), {"u_renpy_solid_color": color})

    def flip(self):
        """
        Called to flip the screen after it's drawn.
        """

        start = time.time()

        renpy.plog(1, "flip")

        try:
            self.gpu.present()
        except RuntimeError as e:
            renpy.display.log.write("Flip failed %r", e)
            renpy.game.interface.display_reset = True

        end = time.time()

        skipped = self.gpu.skipped_frames()
        covered = skipped != self.skipped_seen
        self.skipped_seen = skipped

        if vsync:

            # A swap interval above 1 shows every nth refresh. The present mode only paces one frame per refresh,
            # so hold the rest of the interval here.
            if vsync > 1:
                wait = self.last_flip_end + vsync / self.refresh_rate - end
                if wait > 0:
                    time.sleep(wait)
                    end = time.time()

            # A covered or minimized window gives no drawable, so the present does not block. Hold one refresh.
            elif covered:
                wait = self.last_flip_end + 1.0 / self.refresh_rate - end
                if wait > 0:
                    time.sleep(wait)
                    end = time.time()

            # A present that returns at once because a drawable was free is normal, so no single flip is judged.
            # The guard below looks at the last ten flips.

            frame_times.append(end)

            if len(frame_times) > 10:
                frame_times.pop(0)

                # Nine flip intervals that add up to under 90 percent of nine refreshes: the present is not pacing.
                # Normal Fifo pacing gives nine refreshes, and a short burst of free drawables stays above 90 percent.
                burst = 0.9 * 9 / self.refresh_rate - (frame_times[-1] - frame_times[0])
                if burst > 0:
                    time.sleep(burst / 9)
                    renpy.plog(1, "after broken vsync sleep")

        self.last_flip_end = end

    def draw_screen(self, render_tree, flip=True, screenshot=False):
        """
        Draws the screen. With `screenshot` the tree goes to an offscreen texture, which is returned.
        """

        renpy.plog(1, "start draw_screen")

        t_frame = time.time()

        if renpy.display.video.fullscreen:
            surf = renpy.display.video.render_movie("movie", self.virtual_size[0], self.virtual_size[1])
        else:
            surf = render_tree

        if surf is None:
            return

        # Load all the textures and RTTs.
        self.load_all_textures(surf, IDENTITY)

        clear_r, clear_g, clear_b = renpy.color.Color(renpy.config.gl_clear_color).rgb

        if screenshot:
            w = int(surf.width * self.draw_per_virt)
            h = int(surf.height * self.draw_per_virt)
            target = self.gpu.new_texture(max(w, 1), max(h, 1), False)
            self.gpu.begin_pass(target, (0, 0, max(w, 1), max(h, 1)), (clear_r, clear_g, clear_b, 0.0), True)
            transform = Matrix.screen_projection(surf.width, surf.height)
        else:
            target = None
            x, y, w, h = self.drawable_viewport
            self.gpu.begin_pass(None, (x, y, w, h), (clear_r, clear_g, clear_b, 1.0))
            transform = Matrix.screen_projection(self.virtual_size[0], self.virtual_size[1])

        # Use the context to draw the render tree.
        try:
            _bridge.draw_render(self.gpu, self, surf, int(w), int(h), transform)
        finally:
            self.gpu.end_pass()

        if screenshot:
            return target

        if flip:
            self.flip()
            self.texture_loader.cleanup()

            frame_ms = (time.time() - t_frame) * 1000.0
            self.frame_count += 1
            if frame_ms > 40.0 and self.slow_frames_logged < 20:
                self.slow_frames_logged += 1
                renpy.display.log.write("Slow frame %d: draw and present took %.0f ms.", self.frame_count, frame_ms)

            if not self.first_frame_logged:
                self.first_frame_times.append((time.time() - t_frame) * 1000.0)
                if len(self.first_frame_times) == 30:
                    self.first_frame_logged = True
                    renpy.display.log.write("First 30 frames (draw and present, ms): %s", " ".join("%.0f" % i for i in self.first_frame_times))

    def load_all_textures(self, what, reverse):
        """
        This loads all textures from the surface tree before drawing to
        the actual framebuffer. This is responsible for walking the
        surface tree, and loading framebuffers and texture.

        `reverse`
             A Matrix that transforms from the model space to the drawable space. This is used to determine the
             size of the textures to create when rendering to a texture, in some cases. This is only used to get the
             size right, the position and rotation may or may not be correct.
        """

        if isinstance(what, Surface):
            what = self.load_texture(what)
            self.load_all_textures(what, reverse)
            return

        if isinstance(what, GL2Model):
            what.load()
            return

        # what is a Render.

        r = what

        if r.loaded:
            return

        r.loaded = True

        if r.reverse is not None:
            reverse = reverse * r.reverse

        # Load the child textures.
        # This needs to be outside of r.mesh, as it handles all uniform texture loading,
        # even if uniforms isn't used.

        for c in r.children:
            self.load_all_textures(c[0], reverse)

        # If we have a mesh (or mesh=True), create the GL2Model.
        if r.mesh:

            if (r.mesh is True) and (not r.children):
                return

            if not r.uniforms:
                uniforms = None

            elif r.uniforms_has_render:

                uniforms = dict()

                for k, v in r.uniforms.items():
                    if isinstance(v, renpy.display.render.Render):
                        self.load_all_textures(v, reverse)
                        uniforms[k] = ctex = self.render_to_texture(v, properties=r.properties)
                        uniforms.setdefault(k + "_res", (ctex.texture_width, ctex.texture_height))
                    else:
                        uniforms[k] = v
            else:
                uniforms = r.uniforms

            model = r.cached_model = GL2Model(
                (r.width, r.height),
                None,
                r.shaders,
                uniforms)

            tx, ty = reverse.transform(1, 1)
            oversample = math.hypot(tx, ty) / math.hypot(1, 1)

            oversample = max(1.0, min(oversample, renpy.config.mesh_oversample))

            for i, c in enumerate(r.children):
                model.set_texture(i, self.render_to_texture(c[0], properties=r.properties, oversample=oversample))

            if r.mesh is True:
                tex = model.get_texture(0)
                if tex.width == model.width and tex.height == model.height:
                    model.mesh = tex.mesh
                else:
                    # Otherwise, we need to use a mesh.
                    model.mesh = Mesh2.texture_rectangle(
                        0, 0, r.width, r.height,
                        0, 0, 1, 1)
            else:
                model.mesh = r.mesh

            r.cached_model.properties = r.properties

        elif r.uniforms_has_render:
            for v in r.uniforms.values():
                if isinstance(v, renpy.display.render.Render):
                    self.load_all_textures(v, reverse)
                    self.render_to_texture(v, properties=r.properties)

    def render_to_texture(self, what, alpha=True, properties={}, oversample=1.0):
        """
        Renders `what` to a texture. The texture will have the drawable
        size of `what`.
        """

        if properties is None:
            properties = {}
            need_mipmap = False
        else:
            need_mipmap = properties.get("mipmap", False)

        if isinstance(what, Surface):
            what = self.load_texture(what)

            self.load_all_textures(what, IDENTITY)

        if isinstance(what, Texture):
            if need_mipmap:
                what.add_mipmap()
            return what

        if what.cached_texture is not None:
            if need_mipmap:
                what.cached_texture.add_mipmap()
            return what.cached_texture

        rv = self.texture_loader.render_to_texture(what, properties, oversample)
        what.cached_texture = rv

        return rv

    def is_pixel_opaque(self, what):
        """
        Returns true if the pixel is not 100% transparent.

        `what`
            A 1x1 Render.
        """

        # Load all the textures and RTTs.
        self.load_all_textures(what, IDENTITY)

        if self.pixel_target is None:
            self.pixel_target = self.gpu.new_texture(1, 1, False)

        self.gpu.begin_pass(self.pixel_target, (0, 0, 1, 1), (0.0, 0.0, 0.0, 0.0), True)

        try:
            _bridge.draw_render(self.gpu, self, what, 1, 1, Matrix.screen_projection(1, 1))
        finally:
            self.gpu.end_pass()

        return self.gpu.read_alpha(self.pixel_target)

    def translate_point(self, x, y):
        """
        Translates (x, y) from physical to virtual coordinates.
        """

        # Screen sizes.
        pw, ph = self.physical_size
        vw, vh = self.virtual_size
        vx, vy, vbw, vbh = self.virtual_box

        # Translate to fractional screen.
        x = 1.0 * x / pw
        y = 1.0 * y / ph

        # Translate to virtual size.
        x = vx + vbw * x
        y = vy + vbh * y

        x = int(x)
        y = int(y)

        return x, y

    def untranslate_point(self, x, y):
        """
        Untranslates (x, y) from virtual to physical coordinates.
        """

        # Screen sizes.
        pw, ph = self.physical_size
        vx, vy, vbw, vbh = self.virtual_box

        # Translate from virtual to fractional screen.
        x = (x - vx) / vbw
        y = (y - vy) / vbh

        # Translate from fractional screen to physical.
        x = x * pw
        y = y * ph

        x = int(x)
        y = int(y)

        return x, y

    def mouse_event(self, ev):
        x, y = getattr(ev, 'pos', pygame.mouse.get_pos())
        return self.translate_point(x, y)

    def get_mouse_pos(self):
        x, y = pygame.mouse.get_pos()
        return self.translate_point(x, y)

    def set_mouse_pos(self, x, y):
        x, y = self.untranslate_point(x, y)
        pygame.mouse.set_pos([x, y])

    def screenshot(self, render_tree):
        """
        Draws `render_tree` offscreen and returns it as a Surface with straight (not premultiplied) alpha. Without a
        tree this is the last frame Ren'Py drew (`renpy.game.interface.surftree`), drawn the same way: a presented
        window frame cannot be read back. Before the first frame there is nothing on screen, and the result is a
        transparent surface of the drawable size.
        """

        if render_tree is None:
            interface = getattr(renpy.game, "interface", None)
            render_tree = getattr(interface, "surftree", None)

        if render_tree is None:
            sw, sh = self.drawable_size
            return self._blank_surface(sw, sh)

        target = self.draw_screen(render_tree, flip=False, screenshot=True)

        if target is None:
            return self._blank_surface(*self.drawable_size)

        return self.gpu.read_surface(target, True)

    def _blank_surface(self, w, h):
        target = self.gpu.new_texture(max(int(w), 1), max(int(h), 1), False)
        self.gpu.begin_pass(target, (0, 0, max(int(w), 1), max(int(h), 1)), (0.0, 0.0, 0.0, 0.0), True)
        self.gpu.end_pass()
        return self.gpu.read_surface(target, True)

    def kill_textures(self):
        if self.texture_loader is not None:
            self.texture_loader.cleanup()

    def event_peek_sleep(self):
        pass

    def get_physical_size(self):
        x, y = self.physical_size

        x = int(x / self.dpi_scale)
        y = int(y / self.dpi_scale)

        return (x, y)
