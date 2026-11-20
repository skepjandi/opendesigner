# Store export presets

Generated from the presets the `export` and `validate_export` tools enforce. `export` refuses to write a file that does not meet its preset.

| Preset | What | Size rule | Formats | Alpha | Count |
|---|---|---|---|---|---|
| `apple-iphone-6.9` | App Store iPhone 6.9" display screenshots (portrait) | one of 1260x2736, 1290x2796, 1320x2868 | PNG, JPEG | not allowed | 1-10 |
| `apple-iphone-6.3` | App Store iPhone 6.3" display screenshots (portrait) | one of 1206x2622, 1179x2556 | PNG, JPEG | not allowed | 1-10 |
| `apple-ipad-13` | App Store iPad 13" display screenshots (portrait) | one of 2064x2752, 2048x2732 | PNG, JPEG | not allowed | 1-10 |
| `apple-creative-universal` | App Store creative asset: product page header and search results (16:9) | one of 5244x2950 | PNG | not allowed | 1-1 |
| `apple-creative-header` | App Store creative asset: product page header only (21:9) | one of 3840x1646 | PNG | not allowed | 1-1 |
| `apple-creative-search` | App Store creative asset: search results only (3:2) | one of 3840x2560 | PNG | not allowed | 1-1 |
| `google-phone` | Google Play phone screenshots | each side 320-3840 px, long side at most 2x the short side | PNG, JPEG | not allowed | 2-8 |
| `google-tablet-10` | Google Play 7" and 10" tablet screenshots | width 1080-7680 px | PNG, JPEG | not allowed | 1-8 |
| `google-feature-graphic` | Google Play feature graphic | one of 1024x500 | PNG, JPEG | not allowed | 1-1 |

Sources:

- `apple-iphone-6.9`: https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/ (copied from a reference project, not yet checked against the source)
- `apple-iphone-6.3`: https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/ (copied from a reference project, not yet checked against the source)
- `apple-ipad-13`: https://developer.apple.com/help/app-store-connect/reference/app-information/screenshot-specifications/ (copied from a reference project, not yet checked against the source)
- `apple-creative-universal`: https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications (copied from a reference project, not yet checked against the source)
- `apple-creative-header`: https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications (copied from a reference project, not yet checked against the source)
- `apple-creative-search`: https://developer.apple.com/help/app-store-connect/reference/app-information/creative-assets-specifications (copied from a reference project, not yet checked against the source)
- `google-phone`: https://support.google.com/googleplay/android-developer/answer/9866151 (copied from a reference project, not yet checked against the source)
- `google-tablet-10`: https://support.google.com/googleplay/android-developer/answer/9866151 (copied from a reference project, not yet checked against the source)
- `google-feature-graphic`: https://support.google.com/googleplay/android-developer/answer/9866151 (copied from a reference project, not yet checked against the source)

Stores change their rules. When the user is about to upload, tell them which presets are not yet checked against the source.
