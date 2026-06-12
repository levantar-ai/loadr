Output out/06-distributed.mp4
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
Type "# one controller, two agents, exact fleet-wide percentiles"
Enter
Sleep 800ms
Type "loadr controller --bind 127.0.0.1:7625 --ui-bind 127.0.0.1:6464 >ctl.log 2>&1 &"
Enter
Sleep 1.5s
Type "loadr agent --join 127.0.0.1:7625 --name agent-1 >a1.log 2>&1 &"
Enter
Type "loadr agent --join 127.0.0.1:7625 --name agent-2 >a2.log 2>&1 &"
Enter
Sleep 2.5s
Type "curl -s http://127.0.0.1:6464/api/agents | python3 -m json.tool | grep -E 'name|healthy'"
Enter
Sleep 3s
Type "# submit: 400 iterations split exactly across the fleet"
Enter
Sleep 500ms
Type "loadr run --controller 127.0.0.1:6464 dist.yaml"
Enter
Sleep 14s
Sleep 5s
Hide
Type "kill %1 %2 %3 2>/dev/null"
Enter
