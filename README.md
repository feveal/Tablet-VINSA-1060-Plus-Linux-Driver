## Status

The original driver currently causes severe USB kernel log spam:

usbfs: process did not claim interface before use

Also, the original driver couldn't handle the entire area and
the expansion buttons didn't give 100% of the space, but only about 40%. 
Now it is impossible to change the area on the graphics tablet and 
only 100% of the space will be used.

This results in:
- systemd-journald message loss
- KDE Plasma instability
- random desktop freezes
- The tablet works with an incomplete area
- 
## What is fixed in this fork

- Correct USB interface claiming order
- USB reset performed before interface claim
- Proper interface release on shutdown
- No kernel log spam
- Stable operation under KDE Plasma
- 100% usable area (8192 levels)
- The code has 10000 levels, but even so it works fine

## Root cause

The driver performed USB I/O before claiming the interface,
violating USB userspace contract and triggering kernel warnings.
Just to be clear, I'd like to warn you that the driver 
(the original version from Feveal too) only works with USB 2.0 and higher, 
and if connected to USB 3.0 or higher, the tablet won't respond, 
and the VINSA logo will always be on.

## Build & Run

If you see two warnings while building rust, then everything is fine, I didn't remove unused lines of code.

```bash
cargo build --release
sudo ./target/release/v1060p
```

## The rest of the information is on the github page of the original driver from feveal
