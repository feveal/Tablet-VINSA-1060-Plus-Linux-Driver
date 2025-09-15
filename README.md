 
# Linux Driver for VINSA 1600 Plus Drawing Tablet

Linux driver for the VINSA 1600 Plus drawing tablet with full pressure sensitivity and button support.

The [marvinbelfort](https://github.com/marvinbelfort) driver has been adapted and expanded for this graphics tablet, improving sensitivity and providing two modes of use: one mouse-like, which uses a smaller area of ​​the tablet and is also customizable according to preferences, and another tablet-like mode, which occupies the entire area and offers greater sensitivity for artistic drawing.

- The "B" button toggles between Mouse/Tablet modes.
- The "[" and "]" buttons expand and contract the working area in mouse mode.

## ✨ Features
-  Full pressure support (8192 levels)
-  Adjustable sensitivity settings
-  Mouse/Tablet mode toggle (B button)
-  Dynamically adjustable work area ([ ] buttons)
-  All programmable buttons
-  No sudo required (udev rules included)
-  Desktop launcher with custom icon

## 📦 Installation
You need to have Rust installed previously.

```bash
# Clone the repository
git clone https://github.com/your-username/vinsa-1600-driver.git
cd vinsa-1600-driver

# Build the driver
cargo build --release

# For Install udev rules (no sudo needed)
# Create the file 99-vinsa-tablet.rules
sudo nano /etc/udev/rules.d/99-vinsa-tablet.rules

#  and copy this into it
SUBSYSTEM=="usb", ATTR{idVendor}=="08f2", ATTR{idProduct}=="6811", MODE="0666"
SUBSYSTEM=="input", GROUP="input", MODE="0666"
KERNEL=="uinput", MODE="0666", GROUP="input"

# Reload rules
sudo udevadm control --reload-rules
sudo udevadm trigger

# Make driver executable
chmod +x target/release/v1600p

# In the file VINSA 1600 Plus Tablet.desktop launcher,
change the path where the "v1600p" driver file was placed.
```


## References
- [marvinbelfort](https://github.com/marvinbelfort) - Initial research
- [DIGImend/10moons-tools](https://github.com/DIGImend/10moons-tools) - Expanded mode enablement
- [alex-s-v/10moons-driver](https://github.com/alex-s-v/10moons-driver) - User-space driver approach
