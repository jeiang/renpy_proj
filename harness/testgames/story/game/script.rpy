# Synthetic narrative game. About 60 say lines, three menus, one input, state checked after load.

init python:
    class Crew(object):
        """A plain class instance kept in the save."""
        def __init__(self, name, rank):
            self.name = name
            self.rank = rank
            self.log = ["joined"]

        def promote(self):
            self.rank += 1
            self.log.append("rank%d" % self.rank)

    def snapshot():
        """A text form of every saved value below. Equal text means equal state."""
        return repr((score, list(inventory), sorted(flags.items()), sorted(seen), crew.name, crew.rank,
                     list(crew.log), lead.name, ratio, title, name))

    def ask_name():
        """Ask for a name. A click can end the prompt with a non-text value: ask again, at most three times."""
        for _i in range(3):
            answer = renpy.input("Name?", length=16)
            if isinstance(answer, str) and answer.strip():
                return answer.strip()
        return "Guest"

    def commit():
        """Call after a change: remember what the state must look like."""
        store.state_sig = snapshot()

    def check_state():
        types_ok = (isinstance(score, int) and isinstance(title, str) and isinstance(inventory, list)
                    and isinstance(flags, dict) and isinstance(seen, set) and isinstance(crew, Crew)
                    and isinstance(ratio, float) and isinstance(lead, renpy.character.ADVCharacter))
        if not types_ok:
            raise Exception("resume check: a saved value changed its type")
        if snapshot() != state_sig:
            raise Exception("resume check: saved values differ: %s != %s" % (snapshot(), state_sig))

define ada = Character("Ada", color="#e8b04a")
define bram = Character("Bram", color="#6fb7e8")
define cyra = Character("Cyra", color="#c58fe0", what_italic=True)
define who_is_it = Character("Stranger", color="#9aa0a6")

default score = 0
default title = "Deckhand"
default inventory = ["lamp"]
default flags = {"met_bram": False, "lit": False}
default seen = set()
default crew = Crew("Ada", 1)
default lead = ada
default ratio = 0.5
default menuset = set()
default name = ""
default state_sig = ""

label after_load:
    $ check_state()
    return

image sprite ada = "images/ada.png"
image sprite bram = "images/bram.png"

transform fade_in(x):
    # ATL that rests at shot time: it ends 0.3 s after the show.
    xalign x
    yalign 1.0
    alpha 0.0
    linear 0.3 alpha 1.0

label start:
    scene bg harbor with None
    window hide
    pause 0.1
    window show
    "The ferry bell rang twice over the grey water."
    "Nobody on the pier moved. That was the first strange thing."
    $ name = ask_name()
    $ commit()
    "A clerk looked up from a ledger and wrote something down."
    who_is_it "Welcome, [name]. We have waited for you."
    show sprite ada at fade_in(0.15)
    ada "I am Ada. I keep the lamps on this coast."
    ada "You may call me [lead.name], as the others do."
    ada "Say that again and I will think you came to listen."
    $ score += 1
    $ seen.add("harbor")
    $ commit()
    "Ada's coat was {b}dark blue{/b}, and her lamp was {i}very old{/i}."
    "{color=#e8b04a}Gold light{/color} spilled across the planks."
    "{size=+8}Large{/size} and {size=-6}small{/size} letters shared one line."
    show sprite bram at fade_in(0.85)
    bram "Late again, Ada. The tide will not wait for lamps."
    ada "The tide has never been on time itself."
    bram "[name], you look like someone who can count boats."
    bram "Ten boats went out. How many came back?"
    $ flags["met_bram"] = True
    $ commit()

label first_choice:
    menu:
        "Ask Bram about the boats.":
            $ score += 2
            $ seen.add("boats")
            $ commit()
            bram "Three. The rest are still out, with their lamps lit."
            ada "Or dark. We have not seen a light since noon."
            jump boats_path
        "Follow Ada to the lighthouse.":
            $ score += 3
            $ seen.add("tower")
            $ commit()
            ada "Stay close. The stairs have opinions."
            jump tower_path
        "Look at the water.":
            $ seen.add("water")
            "The water was calm and showed only the sky."
            jump middle

label boats_path:
    scene bg cove with None
    show sprite bram at fade_in(0.5)
    bram "Come to the cove. The wreck lies there."
    "The cove was small and wet, with rocks like loaves of bread."
    bram "My father found a lamp here, once."
    bram "It was lit. It had been in the water a year."
    $ inventory.append("rope")
    $ commit()
    "Bram gave you a coil of {i}rope{/i} and said nothing more."
    cyra "He never says nothing more. He thinks it is deep."
    "A girl with a green scarf sat on the highest rock."
    cyra "I am Cyra. I count the gulls. Today there are none."
    cyra "That is the second strange thing."
    jump middle

label tower_path:
    scene bg tower with None
    show sprite ada at fade_in(0.5)
    "The tower smelled of oil and cold iron."
    ada "Light it, [name]. The match is in the lamp."
    $ flags["lit"] = True
    $ inventory.append("match")
    $ crew.promote()
    $ commit()
    "A thin flame rose and the glass room filled with amber."
    ada "Good. Now they can see us."
    cyra "They can also see that you did not wait for me."
    "Cyra stood on the stairs with a green scarf over her arm."
    jump middle

label middle:
    window hide
    scene bg harbor with None
    window show
    $ ratio = ratio + 0.25
    $ commit()
    "Evening came. The three of you stood on the pier."
    ada "We should decide before dark."

    menu choose_plan:
        set menuset

        "What do we do?"

        "Row out and search.":
            $ score += 5
            $ title = "Rower"
            $ seen.add("row")
            $ commit()
            bram "I will take the oars."
            ada "Then I take the lamp."
        "Wait for the morning tide." if score >= 3:
            $ score += 1
            $ title = "Watcher"
            $ commit()
            cyra "Waiting is also work."
            ada "We will sleep in turns."
        "Ring the bell until someone answers." if "tower" in seen or "boats" in seen:
            $ score += 2
            $ title = "Ringer"
            $ commit()
            bram "The bell is loud. It will wake the fish."
            ada "Then the fish will answer."

    "The night was long, and the sea gave back the sound."
    $ crew.promote()
    $ commit()
    ada "I think I see a light, [name]."
    bram "Where?"
    ada "Where there was none."
    cyra "Third strange thing."

    menu:
        "Tell them the truth.":
            $ score += 4
            "You said you did not know what the lights meant."
            ada "That is the right answer."
        "Make up a story.":
            $ score += 1
            "You told a tale of ghosts and a drowned bell."
            bram "A good tale. Wrong, but good."
        "Stay quiet.":
            $ score += 2
            "You watched the light and said nothing."
            cyra "Quiet is also an answer."

    $ commit()
    jump finale

label finale:
    scene bg tower with None
    show sprite ada at fade_in(0.5)
    "At dawn the light came closer and became a small boat."
    "It carried one lamp, one oar and no passenger."
    ada "It is the tenth boat."
    bram "Then they all came back."
    cyra "Or none of them left."
    window hide
    scene bg harbor with None
    window show
    "Your score was [score], and your title was [title]."
    "Crew rank: [crew.rank]. Items: [inventory]."
    ada "Thank you, [name]. The coast is quiet again."
    $ check_state()
    "The end."
    return
