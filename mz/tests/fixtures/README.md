tone.m4a is a synthetic 440 Hz tone generated for the MP4 decoding regression test:

```sh
ffmpeg -f lavfi -i sine=frequency=440:duration=0.12 -c:a aac -b:a 48k -movflags +faststart tone.m4a
```

No downloaded audio or account data is included.
