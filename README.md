# Linux Driver for VINSA 1060 Plus Drawing Tablet (V5 Cooperativo Wayland)

Linux driver for the VINSA 1060 Plus drawing tablet with full pressure sensitivity and button support. Chipset: `08f2:6811`

## 🚀 Complete Code Modification for Wayland (V5)

- **Wayland Native Architecture**: Modified and optimized for **Kubuntu 26.04** running under Wayland (KWin / Libinput). It exposes three independent virtual devices (`uinput`) to avoid compositor interpretation issues.
- **System Tray Icon Integration**: Added an icon to the taskbar to indicate when the driver is active. This icon dynamically shifts (Mouse 🖱️ / Tablet 🎨) depending on the internal driver state.
- **Krita Cursor Fix**: Uses a standardized device name prefix (`Wacom...`) to force Libinput to treat the virtual injector as a real `TABLET_TOOL` digitalizer, enabling the brush circle and pressure levels in any system language.
- **Multi-language Hotkeys**: Integrates `xkbcommon` to detect the system layout at runtime. It automatically bundles an atomic burst of events with `Alt Gr` (`KEY_RIGHTALT`) if the system is in Spanish to type `[` and `]` correctly, skipping it seamlessly if the system is in English.
- **Anti-USB Flood Filtering**: Implemented an absolute event rejection filter to drop consecutive identical coordinate packets, dropping CPU overhead down to 0% in applications like Google Chrome.
- **Taskbar Control**: Right-clicking the taskbar icon displays a contextual menu to exit the driver cleanly.

## ✨ Features
- Fixed issue where entering the top media area would cause the cursor jump to the bottom screen.
- Full pressure support (8192 levels).
- Adjustable relative mouse speed scaling.
- Mouse/Tablet mode toggle via hardware button.
- Custom desktop launcher integration.

### Buttons mapping
<table>
  <tr>
    <td>
      <img src="driver/resources/botones.png" width="200">
    </td>
    <td>
      <ul>.
        <br></br>
        <li><strong>Button E:</strong> Erase</li>
        <li><strong>Button B:</strong> Toogle Mode (Mouse/Tablet)</li>
        <br></br>
        <li><strong>Button CTRL - :</strong> = Zoom out</li>
        <li><strong>Button CTRL + :</strong> Zoom in</li>
        <br></br>
        <li><strong>Tablet Mode</li>
        <li style="margin-left: 25px;"><strong>Button [ :</strong> Decrease brush size</li>
        <li style="margin-left: 25px;"><strong>Button ] :</strong> Increase brush size</li>
        <br></br>
        <li><strong>Mouse Mode</li>
        <li style="margin-left: 25px;"><strong>Button [ :</strong> Decrease speed mouse</li>
        <li style="margin-left: 25px;"><strong>Button ] :</strong> Increase speed mouse</li>
        <br></br>
        <li><strong>Mouse up:</strong> Scroll up</li>
        <li><strong>TAB:</strong> Toggle full screen</li>
        <br></br>
        <li><strong>Mouse down:</strong> Scroll down</li>
        <li><strong>SPACE:</strong> Space (Pan)</li>
        <br></br>
        <li><strong>CTRL:</strong> CTRL</li>
        <li><strong>ALT:</strong> ALT</li>
        <br></br>
      </ul>
    </td>
  </tr>
</table>

---

## 🛠️ System Requirements

Before compiling, you need to install the required system libraries for **GTK3 interface loop**, **SVG rendering libraries**, and **XKB layout layout context engine**:

```bash
sudo apt update
sudo apt install libxkbcommon-dev libgtk-3-dev pkg-config build-essential
```

---

## 📦 Installation

Make sure you have the Rust toolchain installed on your computer.

### 1. Clone & Build
```bash
# Clone the repository
git clone https://github.com
cd vinsa-1060-driver/driver

# Build the driver binary in release mode
cargo build --release
```

### 2. Configure Udev Rules (Crucial for Non-Sudo execution)
To be able to run the driver as a standard desktop user without root access, create the local rule file:

```bash
sudo nano /etc/udev/rules.d/99-vinsa-tablet.rules
```

Paste the following content inside:

```udev
# VINSA USB Interface permission
SUBSYSTEM=="usb", ATTR{idVendor}=="08f2", ATTR{idProduct}=="6811", MODE="0666"
# Input subsystem permissions
SUBSYSTEM=="input", GROUP="input", MODE="0666"
# Virtual Device injector permission
KERNEL=="uinput", MODE="0666", GROUP="input"
```

Reload the system configurations to apply:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### 3. Setup Desktop Launcher
Give execution privileges to the compiled binary file:
```bash
chmod +x driver/target/release/v1060p
```

Open the `VINSA 1060 Plus Tablet.desktop` file with any text editor and change the `Exec=` and `Icon=` absolute paths to point to your local installation directory. Place it on your Desktop to run it manually on-demand.

---

## References
- [marvinbelfort](https://github.com) - Initial research
- [DIGImend/10moons-tools](https://github.com) - Expanded mode enablement
- [alex-s-v/10moons-driver](https://github.com) - User-space driver approach
