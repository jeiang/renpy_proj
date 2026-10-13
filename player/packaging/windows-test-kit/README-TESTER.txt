urp Windows test kit
====================

This kit tests urp.exe (Universal Ren'Py Player) on your Windows computer.
It runs a set of small synthetic test games and records what happens.
The games were made for this test. They contain no commercial material.


What you need
-------------

- Windows 10 or Windows 11.
- PowerShell. It is part of Windows, so you do not need to install it.
- Enough free disk space to extract the zip. The test writes its results
  into the kit folder.
- Do not use the mouse or keyboard while the test runs. Game windows
  open and close by themselves.


Steps
-----

1. Extract the whole zip to a folder you can write to, such as
   Documents. Do not run it from inside the zip file.
2. Open the folder "urp-windows-test-kit".
3. Double-click run-tests.bat.
4. Wait. A console window shows one line for each step. Game windows
   open and close during the run. This can take a long time.
5. When the console says the test has finished, press a key to close it.
6. The kit folder now contains a file named results-<computer name>-<date
   and time>.zip. Send that file back to the tester contact.


What the test does
------------------

For each test game, the runner:

- checks that the game starts and that its script lints cleanly;
- plays the game for a short time and checks it reaches the menu;
- saves a game, quits, starts again, loads the save, and plays on;
- checks the compatibility report that urp writes for games that need it.
  Some checks are for Ren'Py 7 games and some are for games with patches
  or a hidden engine copy.

Games that need a probe do not run the compatibility checks unless the
probe passed. A SKIP row says why.


What you will get back
----------------------

- summary.txt: one line per step, with PASS, FAIL, WARN, or SKIP.
- results.json: the same data in machine-readable form.
- system.txt: the Windows version and the urp.exe checksum.
- run.log: the full run log.
- logs\: the log files from urp for each game.
- shots\: screenshots, when they could be taken.
- stagefiles\: each step's log.txt and traceback.txt, when urp wrote them.

The results zip contains only the files above. It does not contain your
personal files.


What the test does not check
----------------------------

This kit is a first pass. It does not check:

- how the picture looks, or whether frames are drawn correctly;
- whether sound and video stay in step;
- the full save-file scan, or the deep play-through of each game;
- a comparison with stock Ren'Py output.

A PASS here does not mean the player is fully correct on Windows.


Status of this kit
------------------

The authors have not yet run this kit on a real Windows computer. Its
steps were checked on other systems only. If run-tests.bat does not
start, or the window closes with an error, please send the text of the
error and a photo of the window. Do not try to fix it yourself.


Problems
--------

If something goes wrong, send the results zip if one was made. If no
zip was made, send a photo of the console window.
