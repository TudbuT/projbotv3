# ProjBotV3

Projector Bot V3, written in rust this time.

[V2](https://github.com/tudbut/projectorbotv2_full)

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

