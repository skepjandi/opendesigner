# Formats, sizes and safe zones

Sizes are suggestions: the user may have chosen another in the workspace (`get_brief`); use theirs.
Where they chose nothing, pick the row that matches the destination and say so.

## Video

| destination | size | ratio | fps | length | safe zone (keep text and logos inside) |
|---|---|---|---|---|---|
| Instagram Reel / Story | 1080x1920 | 9:16 | 30 | up to 90 s (Reels) | Meta's ad guide: 14% top, 35% bottom, 6% each side = x 65..1015, y 269..1248 |
| TikTok | 1080x1920 | 9:16 | 30 | up to 10 min; 15 to 60 s works | about top 240, bottom 660, sides 120, plus 300 on the right below y 840 |
| YouTube Short | 1080x1920 | 9:16 (1:1 also) | 30 | up to 3 min | top 288, bottom 672, left 48, right 192 (Google ad zone) |
| YouTube / web / explainer | 1920x1080 | 16:9 | 30 | any | 5% on every side |
| Square feed post | 1080x1080 | 1:1 | 30 | up to 60 s | 6% on every side |
| App Store app preview, iPhone | 886x1920 or 1920x886 | 9:19.5 | 30 max | 15 to 30 s | the whole frame is shown; keep text off the top 8% and bottom 8% |
| App Store app preview, iPad | 1200x1600 or 1600x1200 | 3:4 | 30 max | 15 to 30 s | as above |
| Pinterest / vertical ad | 1080x1920 | 9:16 | 30 | 6 to 15 s | as Reels |

Use the largest of the zones when the video goes to several platforms. Platforms change their
interfaces; the numbers were read on 2026-11-19 from public guides and may drift. Frame sizes are
even numbers.

## Images

| destination | size | notes |
|---|---|---|
| App Store iPhone 6.9" | 1320x2868 (or 1290x2796, 1260x2736) | `apple-iphone-6.9`; no alpha |
| App Store iPhone 6.3" | 1206x2622 | `apple-iphone-6.3` |
| App Store iPad 13" | 2064x2752 | `apple-ipad-13` |
| Google Play phone | 1080x1920 (9:16), 2:1 limit | `google-phone`; no iOS chrome |
| Google Play feature graphic | 1024x500 | text at most a fifth of the area |
| Open Graph / social card | 1200x630 | `web-og` |
| Square post | 1080x1080 | |
| Story / portrait post | 1080x1350 (4:5) or 1080x1920 | |
| Icon | 1024x1024 | no transparency for iOS; keep the motif inside the center 80% |
| Web hero | 1920x1080 | leave the left or right third quiet for text |

`list_export_presets` is the source of truth for store rules; it is checked when you export.

## Choosing when the user did not

Ask which store or platform it is for only if the request does not say. Otherwise decide from the
words ("reel" means 1080x1920 at 30 fps; "App Store screenshots" means the 6.9" preset and, if
they sell on Android too, a Google phone set at 1080x1920 made from its own layout, not a crop).
Mention the choice and that it can change.
