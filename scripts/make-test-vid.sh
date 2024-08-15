#!/bin/sh
ofn=test_vid.mp4
docker run --rm -it -v "$(pwd)":/app -w /app \
  mwader/static-ffmpeg:7.0.2 \
  -y \
  -pixel_format yuv420p -g 30 -c:v libx264 -preset fast -b:v 2.1M -minrate 1.1M -maxrate 2.4M -bufsize 80.0M \
  -filter_complex "\
    smptebars=size=1280x720:rate=60:duration=2 \
  [v1]; \
  [v1]
    rotate=0.066*t \
  [v1]; \
    zoneplate=d=2: ku=512: kv=100: kt2=0: ky2=256: kx2=556: s=1280x720: r=60: yo=0: kt=11  \
    , pad=1280:720: (ow-iw)/2: (oh-ih)/2: violet \
    , setsar=1 \
  [v2]; \
    life=size=320x180: mold=10: death_color=violet: rate=60
    , scale=1280:720  \
    , trim=duration=2 \
  [v3]; \
  [v1][v2][v3]
    concat=n=3: v=1  \
  [v]; \
  [v]
    format=pix_fmts=yuv420p \
  [v]; \
  [v] \
    drawtext=font=mono: text='%{frame_num}': start_number=1: fontcolor=black: fontsize=80: font=inconsolata: x=(w-tw)/2: y=h-(2*lh):     boxcolor=GhostWhite: box=1: boxborderw=8 \
  [v]; \
  [v] \
    drawtext=font=mono: text='%{pts\:flt}':  start_number=1: fontcolor=black: fontsize=80: font=inconsolata: x=(w-tw)/2: y=h-100-(2*lh): boxcolor=GhostWhite: box=1: boxborderw=8 \
 " \
  $ofn
  echo "Wrote $ofn"
