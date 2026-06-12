Output out/05-convert.mp4
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
Type "# escape JMeter: convert a .jmx plan to loadr YAML"
Enter
Sleep 800ms
Type "loadr convert complex.jmx -o from-jmeter.yaml"
Enter
Sleep 3.5s
Type "head -30 from-jmeter.yaml"
Enter
Sleep 4.5s
Type "clear"
Enter
Type "# or bring a k6 script — options, scenarios, checks and all"
Enter
Sleep 800ms
Type "loadr convert scenarios_k6.js -o from-k6.yaml && loadr validate from-k6.yaml"
Enter
Sleep 5s
