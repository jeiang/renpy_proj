"""Execute in the booted legacy player's Python globals, then call capture_*.

Each capture writes <out_dir>/<kind>/manifest.json. LEGACY_TEXT_FONTS is a
JSON array of additional Ren'Py loader font names; every listed font is required.
The two text-testgame variable fonts are also captured when loadable, without
substituting a different font when an explicitly requested font is missing.
"""

import hashlib
import json
import os


def _capture_directory(out_dir, kind):
    path = os.path.join(os.fspath(out_dir), kind)
    os.makedirs(path, exist_ok=True)
    return path


def _capture_json(directory, value):
    with open(os.path.join(directory, "manifest.json"), "w", encoding="utf-8") as stream:
        json.dump(value, stream, ensure_ascii=False, sort_keys=True, indent=2, allow_nan=False)
        stream.write("\n")
    return value


def _capture_pixels(surface):
    width, height, pixels = surface.get_rgba_bytes()
    assert (width, height) == surface.get_size()
    assert len(pixels) == width * height * 4
    return pixels


def _capture_surface_record(directory, name, surface, raw=True):
    pixels = _capture_pixels(surface)
    record = {
        "size": list(surface.get_size()),
        "sha256": hashlib.sha256(pixels).hexdigest(),
        "byte_length": len(pixels),
        "encoding": "RGBA8-row-major-top-down",
        "masks": list(surface.get_masks()),
        "pitch": surface.get_pitch(),
    }
    if raw:
        filename = name + ".rgba"
        with open(os.path.join(directory, filename), "wb") as stream:
            stream.write(pixels)
        record["file"] = filename
    return record


def capture_surface(out_dir):
    """Capture public Rust surface operations on fixed 4x3 RGBA inputs."""
    import renpy.pygame as pygame
    import renpy.pygame.transform as transform

    directory = _capture_directory(out_dir, "surface")
    source_pixels = [
        (0, 0, 0, 0), (255, 0, 0, 1), (0, 255, 0, 127), (0, 0, 255, 128),
        (255, 255, 255, 254), (19, 73, 151, 255), (253, 129, 1, 64), (3, 127, 251, 192),
        (31, 63, 95, 0), (250, 240, 230, 255), (128, 128, 128, 128), (1, 2, 3, 255),
    ]
    destination_pixels = [
        (255, 200, 100, 255), (32, 64, 128, 0), (254, 1, 128, 127), (7, 13, 29, 128),
        (0, 0, 0, 254), (255, 255, 255, 255), (75, 125, 175, 64), (192, 128, 64, 192),
        (2, 4, 8, 255), (16, 32, 64, 1), (99, 149, 199, 128), (250, 249, 248, 255),
    ]

    def make(pixels, alpha=True, masks=None):
        flags = pygame.SRCALPHA if alpha else 0
        surface = pygame.Surface((4, 3), flags, 32) if masks is None else pygame.Surface((4, 3), flags, 32, masks)
        for index, pixel in enumerate(pixels):
            surface.set_at((index % 4, index // 4), pixel)
        return surface

    records = {}

    def save(name, surface, **metadata):
        record = _capture_surface_record(directory, name, surface)
        record.update(metadata)
        records[name] = record

    source = make(source_pixels)
    save("input-source", source)
    save("input-destination", make(destination_pixels))
    flags = [("normal", 0)] + [(name, getattr(pygame, name)) for name in (
        "BLEND_ADD", "BLEND_SUB", "BLEND_MULT", "BLEND_MIN", "BLEND_MAX",
        "BLEND_RGBA_ADD", "BLEND_RGBA_SUB", "BLEND_RGBA_MULT",
        "BLEND_RGBA_MIN", "BLEND_RGBA_MAX", "BLEND_PREMULTIPLIED",
    )]
    for alpha in (True, False):
        target_kind = "rgba" if alpha else "rgb"
        for flag_name, flag in flags:
            target = make(destination_pixels, alpha)
            dirty = target.blit(source, (0, 0), special_flags=flag)
            save("blit-" + target_kind + "-" + flag_name, target, flag=flag, dirty=list(dirty))
            target = make(destination_pixels, alpha)
            dirty = target.fill((71, 133, 219, 97), special_flags=flag)
            save("fill-" + target_kind + "-" + flag_name, target, flag=flag, dirty=list(dirty))

    for name, clip, dest, area in (
        ("negative-destination", None, (-1, -1), None),
        ("negative-source-area", None, (0, 0), (-1, -1, 4, 3)),
        ("clipped-area", (1, 1, 2, 1), (0, 0), (1, 0, 3, 3)),
        ("outside", None, (8, 8), None),
    ):
        target = make(destination_pixels)
        target.set_clip(clip)
        dirty = target.blit(source, dest, area)
        save("blit-" + name, target, clip=list(target.get_clip()), dest=list(dest), area=area, dirty=list(dirty))
    for name, rect, clip in (
        ("partial", (1, 1, 2, 1), None),
        ("negative", (-1, -1, 3, 3), None),
        ("clipped", (0, 0, 4, 3), (1, 1, 2, 1)),
    ):
        target = make(destination_pixels)
        target.set_clip(clip)
        dirty = target.fill((71, 133, 219, 97), rect)
        save("fill-" + name, target, rect=list(rect), clip=list(target.get_clip()), dirty=list(dirty))

    for alpha in (True, False):
        for modulation in (None, 0, 97, 255):
            src = make(source_pixels, alpha)
            src.set_alpha(modulation)
            target = make(destination_pixels)
            target.blit(src, (0, 0))
            save("alpha-{}-{}".format(alpha, modulation), target, alpha=src.get_alpha())
        src = make(source_pixels, alpha)
        src.set_colorkey(source_pixels[5])
        target = make(destination_pixels)
        target.blit(src, (0, 0))
        save("colorkey-{}".format(alpha), target, colorkey=list(src.get_colorkey()))

    for operation in ("scale", "smoothscale"):
        for size in ((8, 6), (2, 2), (7, 5), (1, 1), (4, 3)):
            name = "{}-{}x{}".format(operation, *size)
            save(name, getattr(transform, operation)(source, size))
            target = pygame.Surface(size, pygame.SRCALPHA, 32)
            returned = getattr(transform, operation)(source, size, target)
            save(name + "-destination", target, returns_destination=returned is target)
    for horizontal in (False, True):
        for vertical in (False, True):
            save("flip-{}-{}".format(horizontal, vertical), transform.flip(source, horizontal, vertical))
    for angle in (0, 90, 180, 270, -90, 33):
        save("rotate-{}".format(angle), transform.rotate(source, angle), angle=angle)
    for angle, factor in ((0, 0.5), (0, 2.0), (90, 1.0), (33, 1.25), (-33, 0.75)):
        for smooth in (0, 1):
            save("rotozoom-{}-{}-{}".format(angle, factor, smooth),
                 transform.rotozoom(source, angle, factor, smooth), angle=angle, scale=factor, smooth=smooth)

    parent = make(source_pixels)
    child = parent.subsurface((1, 1, 3, 2))
    nested = child.subsurface((1, 0, 2, 2))
    save("subsurface-before", child, offset=list(child.get_offset()), abs_offset=list(child.get_abs_offset()))
    nested.set_at((0, 1), (213, 17, 91, 43))
    save("subsurface-nested", nested, offset=list(nested.get_offset()), abs_offset=list(nested.get_abs_offset()),
         parent_identity=nested.get_parent() is child, root_identity=nested.get_abs_parent() is parent)
    save("subsurface-parent-after", parent)
    save("subsurface-child-after", child)
    target = make(source_pixels)
    target.blit(target, (1, 1), (0, 0, 3, 2))
    save("overlap-self-blit", target)
    target = make(source_pixels)
    target.blit(target.subsurface((0, 0, 3, 2)), (1, 1))
    save("overlap-subsurface-blit", target)
    for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1), (1, 1)):
        target = make(source_pixels)
        target.scroll(dx, dy)
        save("scroll-{}-{}".format(dx, dy), target)

    save("copy", source.copy())
    save("convert", source.convert())
    save("convert-alpha", source.convert().convert_alpha())
    for name, masks in (
        ("rgba", (0x000000ff, 0x0000ff00, 0x00ff0000, 0xff000000)),
        ("bgra", (0x00ff0000, 0x0000ff00, 0x000000ff, 0xff000000)),
        ("argb", (0x0000ff00, 0x00ff0000, 0xff000000, 0x000000ff)),
        ("rgbx", (0x000000ff, 0x0000ff00, 0x00ff0000, 0)),
    ):
        sample = make(source_pixels, bool(masks[3]), masks)
        mappings = [{"color": list(pixel), "mapped": sample.map_rgb(pixel),
                     "unmapped": list(sample.unmap_rgb(sample.map_rgb(pixel)))} for pixel in source_pixels]
        save("format-" + name, sample, mappings=mappings,
             mapped_pixels=[sample.get_at_mapped((x, y)) for y in range(3) for x in range(4)])
        save("convert-to-" + name, source.convert(sample))
        save("convert-alpha-to-" + name, source.convert_alpha(sample))
    return _capture_json(directory, {"schema_version": 1, "kind": "surface", "cases": records})


def capture_text(out_dir):
    """Capture engine layout, native FT/HB shaping, and individual glyph rasters."""
    import renpy
    import renpy.loader
    import renpy.pygame as pygame
    import renpy.text.font as font_module
    import renpy.text.text as text_module

    directory = _capture_directory(out_dir, "text")
    expected = json.loads(os.environ.get("LEGACY_TEXT_FONTS", "[]"))
    if not isinstance(expected, list) or not all(isinstance(name, str) for name in expected):
        raise ValueError("LEGACY_TEXT_FONTS must be a JSON array of loader font names")
    fonts = ["DejaVuSans.ttf", "DejaVuSans-Bold.ttf"] + expected
    for name in ("fonts/OpenSans-Variable.ttf", "fonts/NotoNaskhArabic-Variable.ttf"):
        if renpy.loader.loadable(name):
            fonts.append(name)
    font_records = {}
    for name in sorted(set(fonts)):
        if not renpy.loader.loadable(name):
            raise FileNotFoundError("Required capture font is missing: " + name)
        with renpy.loader.load(name) as stream:
            data = stream.read()
        font_records[name] = {"sha256": hashlib.sha256(data).hexdigest(), "byte_length": len(data)}

    cases = []

    def add(name, text, width=520, **properties):
        cases.append((name, text, width, properties))

    for size in (11, 14, 18, 34, 44, 72):
        add("size-{}".format(size), "AVATAR WAVE To Yo Wa Ty fi ffi Wg 0123456789", size=size)
    for kern in (-2, 0, 2):
        add("kerning-{}".format(kern), "AV To Wa Yo AVATAR WAVE", size=44, kerning=kern)
    add("kerning-tags", "AV {k=-4}To Wa Yo{/k} AVATAR {k=6}WAVE{/k}", size=44)
    add("ligatures", "office fluffy affinity ffi fl", size=44)
    for tag, value in (("liga", 0), ("kern", 0), ("tnum", 1), ("pnum", 1), ("onum", 1), ("smcp", 1)):
        add("feature-{}-{}".format(tag, value), "office ffi fl AVATAR 1111 0000 8888 Small Caps", size=34,
            font_features={tag: value})
    add("feature-tags", "{feature:liga=0}office fluffy affinity ffi fl{/feature:liga}", size=44)
    add("hebrew", "שלום עולם, מה שלומך היום?", size=34, text_align=1.0)
    add("arabic", "مرحبا بالعالم، كيف حالك اليوم؟", size=34, text_align=1.0)
    add("mixed-hebrew", "שלום world 123 עולם abc (42)", size=32)
    add("mixed-arabic", "Hello مرحبا 2024 بالعالم (ok) end", size=32)
    add("hebrew-wrap", "זהו פסקה ארוכה בעברית שנועדה לבדוק את שבירת השורות מימין לשמאל. כל מילה צריכה להישאר במקומה, והשורות צריכות להתחיל מהצד הימני של התיבה. 2024 Ren'Py.", width=280, size=26, text_align=1.0)
    add("arabic-wrap", "هذه فقرة طويلة بالعربية لاختبار التفاف الأسطر من اليمين إلى اليسار. يجب أن تبقى كل كلمة في مكانها وأن تبدأ الأسطر من الجانب الأيمن للصندوق. 2024 Ren'Py.", width=280, size=28, text_align=1.0, line_spacing=6)
    paragraph = "Justified text spreads the extra space of every line over its gaps, so both edges of the block stay straight. The last line stays short."
    for mode in ("greedy", "tex", "subtitle", "nobreak"):
        add("layout-" + mode, paragraph, width=380, size=22, layout=mode)
    for align in (0.0, 0.5, 1.0):
        add("align-{}".format(align), paragraph, width=380, size=22, text_align=align)
    add("justify", paragraph, width=380, size=22, justify=True)
    add("indent", paragraph, width=380, size=22, first_indent=40, rest_indent=20)
    add("line-spacing", paragraph, width=380, size=22, line_leading=10, line_spacing=-2)
    add("newlines", "First line\nSecond line\n\nFourth line", size=28)
    add("markup", "Small {size=-10}smaller{/size} {size=+12}bigger{/size} {i}Italic{/i} {b}Bold{/b} {u}Under{/u} {s}Strike{/s}", size=28)
    add("bold", "Bold Wg AV fi مرحبا", size=34, bold=True)
    add("italic", "Italic Wg AV fi", size=34, italic=True)
    add("monochrome", "Raster Wg AV fi", size=34, antialias=False)
    add("outline", "Outline Wg AV fi", size=52, outlines=[(3, "#102040", 0, 0)])
    for weight in (300, 650, 800):
        add("axis-wght-{}".format(weight), "Open Sans variable 0123 Wg مرحبا", size=34, axis={"wght": weight})
    add("axis-tags", "{axis:wght=800}Variable weight 800{/axis:wght}", size=34)

    glyph_fields = ("character", "glyph", "variation", "x", "y", "x_offset", "y_offset", "advance", "width",
                    "ascent", "descent", "line_spacing", "split", "draw", "delta_x_adjustment")
    records = []
    original_rtl = renpy.config.rtl
    original_glyphs = text_module.TextSegment.glyphs
    try:
        renpy.config.rtl = True
        for font_name in sorted(font_records):
            for shaper in ("freetype", "harfbuzz"):
                for case_name, body, width, overrides in cases:
                    # FT has no OpenType features or variable-axis interface.
                    if shaper == "freetype" and (case_name.startswith("feature") or case_name.startswith("axis")):
                        continue
                    properties = dict(font=font_name, size=28, color="#ffffff", shaper=shaper,
                                      language="unicode", layout="greedy", hinting="auto", slow_cps=0)
                    properties.update(overrides)
                    segments = []

                    def remember_segment(segment, value, layout, level=0):
                        glyphs = original_glyphs(segment, value, layout, level)
                        segments.append((segment, value, level, glyphs))
                        return glyphs

                    text_module.TextSegment.glyphs = remember_segment
                    displayable = text_module.Text(body, substitute=False, slow=False, **properties)
                    displayable.update()
                    layout = text_module.Layout(displayable, width, 8192, {}, size_only=True, drawable_res=False)
                    text_module.TextSegment.glyphs = original_glyphs
                    segment_records = []
                    for segment, value, level, glyphs in segments:
                        raster_outline = overrides.get("outlines", [(0,)])[0][0]
                        native_font = font_module.get_font(
                            segment.font, segment.size, segment.bold, segment.italic, raster_outline, segment.antialias,
                            segment.vertical, segment.hinting, layout.oversample, segment.shaper,
                            segment.instance, segment.axis, segment.features)
                        glyph_records = []
                        for glyph in glyphs:
                            record = {field: getattr(glyph, field) for field in glyph_fields}
                            # Translate the native laid-out glyph into an isolated fixed canvas;
                            # neither shaping nor positioning is recomputed by this probe.
                            extent = int(segment.size * layout.oversample * 4 + 64)
                            canvas = pygame.Surface((extent, extent), pygame.SRCALPHA, 32)
                            canvas.fill((0, 0, 0, 0))
                            center = extent // 2
                            native_font.draw(canvas, center - glyph.x, center - glyph.y,
                                             (255, 255, 255, 255), [glyph], segment.underline,
                                             segment.strikethrough, None)
                            pixels = _capture_pixels(canvas)
                            record["bitmap"] = {"size": [extent, extent], "origin": [center, center],
                                                "rgba_sha256": hashlib.sha256(pixels).hexdigest(),
                                                "alpha_sha256": hashlib.sha256(pixels[3::4]).hexdigest()}
                            glyph_records.append(record)
                        segment_records.append({"text": value, "level": level, "font": segment.font,
                                                "size": segment.size, "axis": segment.axis, "raster_outline": raster_outline,
                                                "features": segment.features, "shaper": segment.shaper,
                                                "native_type": type(native_font).__module__ + "." + type(native_font).__name__,
                                                "glyphs": glyph_records})
                    # The break flags and coordinates come directly from the real engine layout.
                    paragraphs = [[{field: getattr(glyph, field) for field in glyph_fields}
                                   for glyph in paragraph] for paragraph in layout.paragraph_glyphs]
                    records.append({"id": "{}:{}:{}".format(font_name, shaper, case_name), "text": body,
                                    "properties": properties, "width": width, "size": list(layout.size),
                                    "baseline": layout.baseline, "oversample": layout.oversample,
                                    "paragraphs": paragraphs, "segments": segment_records})
    finally:
        text_module.TextSegment.glyphs = original_glyphs
        renpy.config.rtl = original_rtl
    return _capture_json(directory, {"schema_version": 1, "kind": "text", "rtl": True,
                                     "fonts": font_records, "cases": sorted(records, key=lambda item: item["id"])})
