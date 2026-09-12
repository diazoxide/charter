# SVG captures of the chat strip: slots.TAB_SPINNER cycles ✢✶✻✶, and ✻ ren

_2026-09-11 01:41 · persistent_

SVG captures of the chat strip: slots.TAB_SPINNER cycles ✢✶✻✶ and the current chat is marked *. Rendered at the width GitHub shows the README (<img width=830>, headless Chrome 1x and 2x), BOTH ✻ and ✶ read as that * — a still on either shows two current chats (✶ got past a 2x zoom check and was caught by a reviewer at display size). ✢ reads as +, and all three are eaw=N width 1, so capture-frame.sh --full waits for ✢. Check glyph legibility at README display width, not only zoomed. Headless Chrome --screenshot writes the PNG and then never exits: background it, poll for the file, kill its own $! (never pkill). qlmanage -t returns a square thumbnail (1206x1206 for a 1206x663 SVG), so it is no evidence about a wide capture's right edge.
