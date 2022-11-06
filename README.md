# ProjBotV3

Projector Bot V3, written in rust this time.

[V2](https://github.com/tudbut/projectorbotv2_full)

## Quick start

1. Download the executable from https://github.com/tudbut/projbotv3/releases/latest
2. Download ffmpeg using [the official but complicated process](https://ffmpeg.org/), your package manager of choice, or from the releases page of this repo.
3. Download a video and rename it to `vid.mp4`
4. Put all these downloaded files into the same folder
5. Start a CMD or terminal, `cd WHERE_YOU_PUT_THE_FILES` and run the executable using `./projbotv3 TOKEN_HERE`.

## How to build it

First, install this by [installing the rust toolchain](https://rustup.rs) and then running
`cargo install --git ` followed by the link to this repo. 

Afterwards, you can use it like this (linux)
```
$ # the ytdl command is just an example
$ ytdl -q 18 -o vid.out https://youtu.be/FtutLA63Cp8 # download a video to vid.mp4
$ projbotv3 $(cat bot-token) # assuming there is a file called bot-token containing the bot token
```
(windows)
`projbotv3 BOT_TOKEN_HERE` (sadly, windows does not support putting that in files)

The bot will now convert the video into its preferred format and then connect to discord.

### Useful commands

~~So far, V3 isn't fully automatically converting the images. Either use V2 for that, or run
these commands and figure out a way to merge multiple pngs to a gif.~~
It is now able to do all this automatically.

```
ffmpeg -i vid.mp4 -vf fps=fps=30 -deadline realtime vid_30fps.mp4
ffmpeg -i vid_30fps.mp4 -vf scale=240:180,setsar=1:1 -deadline realtime vid/%0d.png
# at this point a merger for multiple pngs to a gif is needed
ffmpeg -i vid.mp4 -deadline realtime aud.opus && mv aud.opus aud_encoded
```

