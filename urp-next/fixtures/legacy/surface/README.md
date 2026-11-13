# Legacy surface

Raw RGBA8 outputs of fixed small pygame Surface operations, plus pixel format and dirty-rectangle metadata.

Regenerate (fresh WORK; `$URP_NEXT` is this tree, `$OLD_REPO` the old checkout with the release build, SDKs and built synthetic games):

```sh
cd "$OLD_REPO" && "$URP_NEXT/tools/capture-legacy/environment.sh" "$URP_NEXT/tools/capture-legacy/regenerate-all.sh" . "$WORK" "$WORK/package"
```

Source: `ec0381aef7453d2f89c92571b0fa226d0b6a33e1`. Host: `artemis`, x86_64. Renderer: wgpu-vulkan-lavapipe.
Two separate process captures: byte-identical.
