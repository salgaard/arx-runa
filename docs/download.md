# Download

Arx Runa is free and open source. All releases are available on GitHub.

**[→ View all releases on GitHub](https://github.com/salgaard/arx-runa/releases)**

---

## Windows

Download the **NSIS installer** (`.exe`) from the latest release and run it.

> **SmartScreen warning**: Windows may show "Windows protected your PC" because the app is not code-signed. Click **More info**, then **Run anyway**. This is normal for open-source software distributed outside the Microsoft Store.

---

## macOS

Download the **disk image** (`.dmg`) from the latest release, open it, and drag Arx Runa to your Applications folder.

> **Gatekeeper warning**: On first launch, macOS will block the app because it is not notarized. To open it:
> 1. Right-click (or Control-click) the app icon → **Open**
> 2. Click **Open** in the dialog that appears
>
> Alternatively: System Settings → Privacy & Security → scroll down → **Open Anyway**

---

## Linux

Two formats are available from the latest release:

- **AppImage** (`.AppImage`) — portable, runs on any distribution without installation:
  ```sh
  chmod +x arx-runa_*.AppImage
  ./arx-runa_*.AppImage
  ```
- **Debian package** (`.deb`) — for Ubuntu, Debian, and derivatives:
  ```sh
  sudo dpkg -i arx-runa_*.deb
  ```

---

## Source

Build from source by cloning the [repository](https://github.com/salgaard/arx-runa) and following the instructions in the README.
