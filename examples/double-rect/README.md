# double-rect

Demonstration of buffer write issue with DX11 + Intel HD.
Frame rendering starts before the end of uniform buffer update.
That results in a glitches.
If you see only one rectangle then you're OK.

## To Run

```
cargo run --example double-rect
```
