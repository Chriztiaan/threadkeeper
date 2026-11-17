# Threadkeeper screenshots

These are actual native Rune views with fictional demo data. No live credentials, customer information, or personal notes are captured.

- [Comment previews and collapse controls](comments.png)
- [VIP settings and notes](vips.png)
- [Welcome](welcome.png)
- [Follow-up and VIP highlighting](follow-up.png)
- [Pull request](pull-request.png)
- [Developer work and highlighted replies](developer-work.png)
- [GitHub connection](github-setup.png)
- [Animated screenshot tour](threadkeeper-tour.gif) — three views, three seconds each; a looping slideshow, not a live screen recording.

Regenerate on macOS:

```sh
cargo build -p threadkeeper --offline
python3 desktop-rune/scripts/screenshots.py
```
