# Render an animated demo GIF with VHS (https://github.com/charmbracelet/vhs):
#   brew install vhs        # (needs ffmpeg + ttyd)
#   vhs demo.tape           # → docs/demo.gif
#
# Records a real Tessera session against a sample repo. Adjust the symbol names
# to match whatever repo you point it at.

Output docs/demo.gif

Set Theme "Catppuccin Mocha"
Set FontSize 18
Set Width 1100
Set Height 640
Set Padding 24
Set TypingSpeed 40ms

Hide
Type "tessera index . >/dev/null 2>&1" Enter
Show

Type "tessera index ." Sleep 400ms Enter Sleep 1.2s
Type "tessera impact BaseWorker" Sleep 400ms Enter Sleep 2s
Type "tessera validate findByid" Sleep 400ms Enter Sleep 2s
Type "tessera connect handler loadUser" Sleep 400ms Enter Sleep 2s
Type "tessera export --from handler --format mermaid" Sleep 400ms Enter Sleep 2.5s
Sleep 1s
