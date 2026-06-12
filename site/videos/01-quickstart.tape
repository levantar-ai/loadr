Output out/01-quickstart.mp4
Set Shell "bash"
Set FontSize 20
Set Width 1600
Set Height 900
Set Padding 24
Set TypingSpeed 30ms
Set Theme {"background":"#0a0a0e","foreground":"#d6dae3","cursor":"#ef4444","black":"#16161c","red":"#ef4444","green":"#4ade80","yellow":"#fbbf24","blue":"#60a5fa","magenta":"#f472b6","cyan":"#22d3ee","white":"#d1d5db","brightBlack":"#52525e","brightRed":"#f87171","brightGreen":"#86efac","brightYellow":"#fde047","brightBlue":"#93c5fd","brightMagenta":"#f9a8d4","brightCyan":"#67e8f9","brightWhite":"#f4f4f5","selection":"#7f1d1d"}
Hide
Type "cd /tmp/loadr-demo && export PATH=/home/andy-rea/development/loadr.io/target/debug:$PATH && export PS1='\[\e[1;31m\]loadr ❯\[\e[0m\] ' && clear"
Enter
Show
Type "# a complete load test in 20 lines of YAML"
Enter
Sleep 800ms
Type "cat quickstart.yaml"
Enter
Sleep 4.5s
Type "clear"
Enter
Type "# validate first — precise diagnostics, then run it"
Enter
Sleep 500ms
Type "loadr validate quickstart.yaml"
Enter
Sleep 2.5s
Type "loadr run quickstart.yaml"
Enter
Sleep 11s
Sleep 6s
