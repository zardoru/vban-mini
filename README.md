vban-mini
=====
vban-mini is a command-line utility for transmitting and receiving audio using the vban protocol utilized by tools like [VoiceMeeter](https://vb-audio.com/Voicemeeter/index.htm). 
If you are comfortable with command line interfaces and generally know how to use VoiceMeeter this tool should be fairly intuitive to use. Use the ```--help``` option for reference.

`--transmit-ip` Allows you to set the IP to transmit to. If unset, the tool will receive a signal instead.

`--bind-ip` Allows you to set the IP you bind the socket to. If you're not sure what it does, don't touch it. If you want to change the port but don't know what IP to use, use `0.0.0.0:<your-port>`.

`--stream-name` Allows you to filter the signal of the stream you want to actually use. By default it is `Stream1`.

The audio device used is the one set by default in your system. This won't be changed, as it seems to be a limitation of the rust bindings for portaudio.