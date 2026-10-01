# Text rendering test game. Every case is a still screen with text only, followed by one say line (say N holds the case).
# Fonts: DejaVuSans.ttf and DejaVuSans-Bold.ttf ship in renpy/common. OpenSans-Variable.ttf and NotoNaskhArabic-Variable.ttf
# (OFL) come from fetch.sh through build.py into game/fonts/. When they are missing, the game uses DejaVu instead.

define config.name = "Text test"
define config.save_directory = "text-test"
define config.screen_width = 1280
define config.screen_height = 720
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Text test"
define config.developer = False
define config.rtl = True

init python:
    HAVE_VAR = renpy.loadable("fonts/OpenSans-Variable.ttf")
    HAVE_NASKH = renpy.loadable("fonts/NotoNaskhArabic-Variable.ttf")
    DJ = "DejaVuSans.ttf"
    DJB = "DejaVuSans-Bold.ttf"
    OS = "fonts/OpenSans-Variable.ttf" if HAVE_VAR else DJ
    NK = "fonts/NotoNaskhArabic-Variable.ttf" if HAVE_NASKH else DJ

style tc_text is default:
    color "#f2f2f2"
    size 40
    font "DejaVuSans.ttf"

style tc_label is default:
    color "#8fa8c0"
    size 14
    font "DejaVuSans.ttf"
    min_width 120

style tc_bright_box:
    background Solid("#f4f4e8")
    padding (10, 4)

style tc_dark_box:
    background Solid("#0c0c10")
    padding (10, 4)

style tc_mid_box:
    background Solid("#6a7f95")
    padding (10, 4)

# A row: a small label on the left, then the sample. `**props` reach the sample text.
screen tc_row(label, body, **props):
    hbox:
        spacing 14
        text label style "tc_label" yalign 0.5
        text body:
            style "tc_text"
            properties props

screen tc_frame():
    add Solid("#1e2832")

init python:
    # Without gui.rpy no theme asks for a screen based main menu: ask for it.
    layout.screen_main_menu()

screen main_menu():
    tag menu
    add Solid("#1e2832")
    text "Text test" size 72 color "#f2f2f2" xalign 0.5 ypos 180
    textbutton "Start" action Start() xalign 0.5 ypos 360 text_size 40 text_color "#ffd070"

screen say(who, what):
    window:
        id "window"
        background Solid("#000000d0")
        xfill True
        ysize 150
        yalign 1.0
        if who:
            text who size 26 color "#ffd070" xpos 40 ypos 12
        text what id "what" xpos 40 ypos 52 xsize 1200 size 26 color "#ffffff"

# 1: kerning, pair by pair, with the font's own kerning and the kerning property and the {k} tag.
screen tc_kerning():
    use tc_frame
    vbox:
        xpos 30
        ypos 12
        spacing 4
        use tc_row("DejaVu", "AV To Wa Yo AVATAR WAVE", font=DJ, size=44)
        use tc_row("DejaVu +2", "AV To Wa Yo AVATAR WAVE", font=DJ, size=44, kerning=2)
        use tc_row("DejaVu -2", "AV To Wa Yo AVATAR WAVE", font=DJ, size=44, kerning=-2)
        use tc_row("Open Sans", "AV To Wa Yo AVATAR WAVE", font=OS, size=44)
        use tc_row("Open Sans +2", "AV To Wa Yo AVATAR WAVE", font=OS, size=44, kerning=2)
        use tc_row("Open Sans -2", "AV To Wa Yo AVATAR WAVE", font=OS, size=44, kerning=-2)
        use tc_row("{{k=-4}", "AV {k=-4}To Wa Yo{/k} AVATAR {k=6}WAVE{/k}", font=OS, size=44)
        use tc_row("small 18", "AVATAR WAVE To Yo Wa Ty Pa Te Ve Ry", font=OS, size=18)

# 2: ligatures and OpenType features, and the variable axes through tags.
screen tc_features():
    use tc_frame
    vbox:
        xpos 30
        ypos 12
        spacing 4
        use tc_row("DejaVu", "office fluffy affinity ffi fl", font=DJ, size=44)
        use tc_row("Open Sans", "office fluffy affinity ffi fl", font=OS, size=44)
        use tc_row("liga off", "{feature:liga=0}office fluffy affinity ffi fl{/feature:liga}", font=OS, size=44)
        use tc_row("tnum", "{feature:tnum=1}1111 0000 8888 1.11{/feature:tnum}", font=OS, size=44)
        use tc_row("pnum", "{feature:pnum=1}1111 0000 8888 1.11{/feature:pnum}", font=OS, size=44)
        use tc_row("onum", "{feature:onum=1}0123456789 2024{/feature:onum}", font=OS, size=44)
        use tc_row("smcp", "{feature:smcp=1}Small Caps Sample{/feature:smcp}", font=OS, size=44)
        use tc_row("kern off", "{feature:kern=0}AVATAR WAVE To Yo{/feature:kern}", font=OS, size=44)

# 3: right to left, shaping and mixed direction. config.rtl is True, so the bidi algorithm runs.
screen tc_rtl():
    use tc_frame
    vbox:
        xpos 30
        ypos 12
        spacing 4
        xsize 1220
        use tc_row("Hebrew DejaVu", "שלום עולם", font=DJ, size=34, xsize=1050, text_align=1.0)
        use tc_row("Hebrew Open Sans", "שלום עולם, מה שלומך היום?", font=OS, size=34, xsize=1050, text_align=1.0)
        use tc_row("Arabic DejaVu", "مرحبا بالعالم", font=DJ, size=34, xsize=1050, text_align=1.0)
        use tc_row("Arabic Naskh", "مرحبا بالعالم، كيف حالك اليوم؟", font=NK, size=34, xsize=1050, text_align=1.0)
        use tc_row("Naskh Bold", "السلام عليكم ورحمة الله", font=NK, size=34, bold=True, xsize=1050, text_align=1.0)
        use tc_row("mixed 1", "שלום world 123 עולם abc", font=OS, size=32, xsize=1050)
        use tc_row("mixed 2", "Hello مرحبا 2024 بالعالم (ok) end", font=NK, size=32, xsize=1050)
        use tc_row("mixed 3", "العدد 3.14 و 2,500 و Ren'Py 8 ثابت", font=NK, size=32, xsize=1050, text_align=1.0)
        use tc_row("neutrals", "(1) שלום - 42, עולם! [[x] 56%", font=OS, size=34, xsize=1050)

# 4: a long wrapped RTL paragraph in each script, and an LTR one for comparison.
screen tc_rtl_wrap():
    use tc_frame
    hbox:
        xpos 30
        ypos 14
        spacing 20
        vbox:
            spacing 10
            text "Hebrew, 520 px" style "tc_label"
            text "זהו פסקה ארוכה בעברית שנועדה לבדוק את שבירת השורות מימין לשמאל. כל מילה צריכה להישאר במקומה, והשורות צריכות להתחיל מהצד הימני של התיבה. אחרי המילים יש גם מספרים כמו 2024 ו-3.14 וגם מילה באנגלית: Ren'Py." style "tc_text" font OS size 26 xsize 520 text_align 1.0
        vbox:
            spacing 10
            text "Arabic, 520 px" style "tc_label"
            text "هذه فقرة طويلة بالعربية لاختبار التفاف الأسطر من اليمين إلى اليسار. يجب أن تبقى كل كلمة في مكانها وأن تبدأ الأسطر من الجانب الأيمن للصندوق. وبعد الكلمات توجد أرقام مثل 2024 و 3.14 وكلمة بالإنجليزية: Ren'Py." style "tc_text" font NK size 28 xsize 520 text_align 1.0 line_spacing 6
        vbox:
            spacing 10
            text "Latin, 200 px" style "tc_label"
            text "A short Latin paragraph beside two right to left ones, wrapped at 200 px." style "tc_text" font OS size 22 xsize 200

# 5: outlines on bright, dark and mid backgrounds.
screen tc_outlines():
    use tc_frame
    vbox:
        xpos 30
        ypos 10
        spacing 8
        hbox:
            spacing 14
            text "bright" style "tc_label" yalign 0.5
            frame:
                style "tc_bright_box"
                text "Outline 2 px" style "tc_text" size 52 color "#f8f8f8" outlines [(2, "#000000", 0, 0)]
            frame:
                style "tc_bright_box"
                text "Red 4 px" style "tc_text" size 52 color "#ffffff" outlines [(4, "#c01818", 0, 0)]
        hbox:
            spacing 14
            text "dark" style "tc_label" yalign 0.5
            frame:
                style "tc_dark_box"
                text "Shadow 3,3" style "tc_text" size 52 color "#ffffff" outlines [(0, "#00000080", 3, 3)]
            frame:
                style "tc_dark_box"
                text "Glow 3 px" style "tc_text" size 52 color "#101010" outlines [(3, "#40d0ff", 0, 0)]
        hbox:
            spacing 14
            text "mid" style "tc_label" yalign 0.5
            frame:
                style "tc_mid_box"
                text "Two outlines" style "tc_text" size 52 color "#ffe070" outlines [(6, "#102040", 0, 0), (2, "#ffffff", 0, 0)]
            frame:
                style "tc_mid_box"
                text "Open Sans 2" font OS size 52 color "#ffffff" outlines [(2, "#000000", 0, 1)]
        hbox:
            spacing 14
            text "{{outlinecolor}" style "tc_label" yalign 0.5
            frame:
                style "tc_dark_box"
                text "{outlinecolor=#f00}Red {outlinecolor=#0f0}Green {outlinecolor=#4af}Blue{/outlinecolor}{/outlinecolor}{/outlinecolor}" style "tc_text" size 52 color "#ffffff" outlines [(3, "#ffffff", 0, 0)]
            frame:
                style "tc_dark_box"
                text "Bold DejaVu" font DJB size 52 color "#ffffff" outlines [(1, "#c04040", 0, 0)]
        hbox:
            spacing 14
            text "arabic" style "tc_label" yalign 0.5
            frame:
                style "tc_bright_box"
                text "مرحبا بالعالم" font NK size 52 color "#ffffff" outlines [(3, "#101830", 0, 0)]

# 6: extreme sizes, with and without outlines.
screen tc_sizes():
    use tc_frame
    vbox:
        xpos 30
        ypos 6
        spacing 2
        text "Large 72 Wg" style "tc_text" size 72 font OS
        text "Large 72 outline" style "tc_text" size 72 color "#ffffff" outlines [(3, "#a02020", 0, 0)]
        text "Large 72 ВОЛЯ fi" style "tc_text" size 72 font DJ
        text "Small 14 px: The quick brown fox jumps over the lazy dog 0123456789" style "tc_text" size 14 font DJ
        text "Small 14 px: The quick brown fox jumps over the lazy dog 0123456789" style "tc_text" size 14 font OS
        text "Small 14 outline: The quick brown fox jumps over the lazy dog" style "tc_text" size 14 font OS outlines [(1, "#000000", 0, 0)]
        text "Small 11 px: The quick brown fox jumps over the lazy dog 0123456789" style "tc_text" size 11 font OS
        text "Size 20 22 24 28 36" style "tc_text" size 20 font OS

# 7: variable fonts. Ren'Py 8.5 has the axis, instance and font_features style properties and tags.
screen tc_variable():
    use tc_frame
    vbox:
        xpos 30
        ypos 8
        spacing 3
        use tc_row("default", "Open Sans variable 0123 Wg", font=OS, size=34)
        use tc_row("bold=True", "Open Sans variable 0123 Wg", font=OS, size=34, bold=True)
        use tc_row("wght 300", "{axis:wght=300}Open Sans variable 0123 Wg{/axis:wght}", font=OS, size=34)
        use tc_row("wght 800", "{axis:wght=800}Open Sans variable 0123 Wg{/axis:wght}", font=OS, size=34)
        use tc_row("wdth 75", "{axis:wdth=75}Open Sans variable 0123 Wg{/axis:wdth}", font=OS, size=34)
        use tc_row("style axis", "wght 650 wdth 85 Open Sans", font=OS, size=34, axis={"wght": 650, "wdth": 85})
        use tc_row("sweep", "{size=12}12{/size} {size=16}16{/size} {size=20}20{/size} {size=26}26{/size} {size=34}34{/size} {size=44}44{/size}", font=OS, size=34)
        use tc_row("Naskh", "مرحبا بالعالم", font=NK, size=34, xsize=500, text_align=1.0)
        use tc_row("Naskh 700", "{axis:wght=700}مرحبا بالعالم{/axis:wght}", font=NK, size=34, xsize=500, text_align=1.0)

# 8: markup tags.
screen tc_misc():
    use tc_frame
    vbox:
        xpos 30
        ypos 6
        spacing 3
        use tc_row("size", "Small {size=-10}smaller{/size} base {size=+12}bigger{/size} {size=60}60{/size}", font=DJ, size=28)
        use tc_row("color", "{color=#f44}Red{/color} {color=#4f4}Green{/color} {color=#48f}Blue{/color} {color=#fc0f}Gold{/color}", font=DJ, size=28)
        use tc_row("u s i b", "{u}Underline{/u} {s}Strike{/s} {i}Italic{/i} {b}Bold{/b} {u}{i}{b}All{/b}{/i}{/u}", font=OS, size=28)
        use tc_row("a", "A {a=http://example.invalid/one}hyperlink{/a} and {a=jump:none}another{/a} here", font=OS, size=28)
        use tc_row("alpha", "{alpha=0.25}Faint{/alpha} {alpha=0.5}Half{/alpha} {alpha=1.0}Full{/alpha}", font=OS, size=28)
        use tc_row("vspace", "Line one{vspace=14}Line two after 14 px", font=OS, size=28)
        use tc_row("space", "A{space=40}B{space=10}C", font=OS, size=28)
        use tc_row("ruby", "{rb}Base{/rb}{rt}top{/rt} text {rb}below{/rb}{art}alt{/art} text", font=OS, size=28, ruby_line_leading=14)
        use tc_row("plain", "{b}{i}plain{plain} resets{/plain}{/i}{/b} {font=DejaVuSans-Bold.ttf}font tag{/font}", font=OS, size=28)
        hbox:
            spacing 14
            text "min_width" style "tc_label" yalign 0.5
            frame:
                background Solid("#44546a")
                padding (0, 0)
                text "ab" style "tc_text" font OS size 28 min_width 300 text_align 0.5

# 9: paragraph layout. justify, textalign, indent, first_indent, rest_indent.
screen tc_layout():
    use tc_frame
    $ para = "Justified text spreads the extra space of every line over its gaps, so both edges of the block stay straight. The last line stays short."
    hbox:
        xpos 30
        ypos 14
        spacing 24
        vbox:
            spacing 6
            text "justify" style "tc_label"
            text para style "tc_text" font OS size 22 xsize 380 justify True
            text "text_align 0.5" style "tc_label"
            text para style "tc_text" font OS size 22 xsize 380 text_align 0.5
        vbox:
            spacing 6
            text "text_align 1.0" style "tc_label"
            text para style "tc_text" font OS size 22 xsize 380 text_align 1.0
            text "first_indent 40, rest_indent 20" style "tc_label"
            text para style "tc_text" font OS size 22 xsize 380 first_indent 40 rest_indent 20
        vbox:
            spacing 6
            text "layout nobreak" style "tc_label"
            text "no break at spaces inside this line" style "tc_text" font OS size 22 layout "nobreak"
            text "line_leading 10" style "tc_label"
            text para style "tc_text" font OS size 22 xsize 380 line_leading 10 line_spacing -2

# 10: a long wrapped paragraph with layout "subtitle" (balanced lines), against "tex" and default greedy breaking.
screen tc_subtitle():
    use tc_frame
    $ para = "A subtitle layout balances the length of its lines, so the last line is not a single stranded word at the end of a long sentence."
    vbox:
        xpos 30
        ypos 14
        spacing 6
        text "layout subtitle" style "tc_label"
        text para style "tc_text" font OS size 30 xsize 760 layout "subtitle" text_align 0.5
        text "layout tex" style "tc_label"
        text para style "tc_text" font OS size 30 xsize 760 layout "tex"
        text "layout greedy (default)" style "tc_label"
        text para style "tc_text" font OS size 30 xsize 760

define cn = Character(None, what_font=OS, what_kerning=1.5, what_outlines=[(1, "#203050", 0, 0)])
define cr = Character(None, what_font=NK, what_size=30, what_text_align=1.0)

label start:
    scene black
    "Text rendering test. Each screen shows one group of cases. Advance with a click."

    show screen tc_kerning
    cn "Kerning: pairs in two fonts, with the kerning property and the k tag. This line uses kerning 1.5 with a thin outline."
    hide screen tc_kerning

    show screen tc_features
    "Ligatures and OpenType features: liga, tnum, pnum, onum, smcp and kern."
    hide screen tc_features

    show screen tc_rtl
    cr "مرحبا بالعالم. שלום עולם. Mixed line with 42 digits."
    hide screen tc_rtl

    show screen tc_rtl_wrap
    "Long right to left paragraphs wrap inside a 520 px box."
    hide screen tc_rtl_wrap

    show screen tc_outlines
    "Outlines: width, color, offset, two layers, the outlinecolor tag, and a 4 px outline."
    hide screen tc_outlines

    show screen tc_sizes
    "Large text at 72 px and small text at 14 px and 11 px."
    hide screen tc_sizes

    show screen tc_variable
    "Variable fonts: the default instance, bold, the axis tag, the axis property and a size sweep."
    hide screen tc_variable

    show screen tc_misc
    "Markup: size, color, underline, strike, italic, bold, hyperlink, alpha, vspace, space, ruby and min_width."
    hide screen tc_misc

    show screen tc_layout
    "Paragraph layout: justify, text_align, indents, nobreak and line leading."
    hide screen tc_layout

    show screen tc_subtitle
    "A long paragraph with layout subtitle, tex and greedy breaks."
    hide screen tc_subtitle

    show screen tc_rtl
    cr "هذا السطر يعرض نصا عربيا مرة أخرى بعد حفظ اللعبة."
    "A second look at the right to left screen, after the save point."
    hide screen tc_rtl

    show screen tc_kerning
    cn "The kerning screen again, with the same kerning and outline in the say window."
    "Both screens match the first time they were shown."
    hide screen tc_kerning

    show screen tc_outlines
    "The outline screen again."
    "Three more lines remain."
    hide screen tc_outlines

    "Two more lines remain."
    "The last line. The test game ends here."
    return
