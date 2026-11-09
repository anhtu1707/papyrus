import os
import subprocess
import sys
import zipfile
import time

wix_zip = r"F:\Benali\wix311.zip"
wix_dir = r"F:\Benali\wix311"
installer_dir = r"F:\Benali\papyrus\installer"
wxs_file = os.path.join(installer_dir, "Papyrus.wxs")
wixobj_file = os.path.join(installer_dir, "Papyrus.wixobj")
output_msi = r"F:\Benali\Papyrus-v0.3.7-x64.msi"

print("Waiting for WiX download to finish...")
while True:
    if os.path.exists(wix_zip):
        size = os.path.getsize(wix_zip)
        # Expected size ~34,670,186 bytes
        if size >= 34670000:
            print(f"Download complete: {size} bytes")
            break
        else:
            print(f"Downloading: {size / (1024*1024):.2f} MB / 33.06 MB...")
    time.sleep(3)

if not os.path.exists(os.path.join(wix_dir, "candle.exe")):
    print("Extracting WiX toolset...")
    os.makedirs(wix_dir, exist_ok=True)
    with zipfile.ZipFile(wix_zip, 'r') as z:
        z.extractall(wix_dir)
    print("WiX 3.11 extracted successfully.")

candle_exe = os.path.join(wix_dir, "candle.exe")
light_exe = os.path.join(wix_dir, "light.exe")

print("Compiling WiX source (candle.exe)...")
candle_cmd = [
    candle_exe,
    "-arch", "x64",
    "-out", wixobj_file,
    wxs_file
]
res = subprocess.run(candle_cmd, capture_output=True, text=True)
print(res.stdout)
if res.stderr:
    print("Candle errors:", res.stderr)
if res.returncode != 0:
    print("Candle failed with exit code", res.returncode)
    sys.exit(1)

print("Linking MSI package (light.exe)...")
light_cmd = [
    light_exe,
    "-sval", # suppress validation for faster linking
    "-ext", "WixUIExtension",
    "-out", output_msi,
    wixobj_file
]
res = subprocess.run(light_cmd, capture_output=True, text=True)
print(res.stdout)
if res.stderr:
    print("Light errors:", res.stderr)
if res.returncode != 0:
    print("Light failed with exit code", res.returncode)
    sys.exit(1)

print("\n==========================================")
print(f"MSI CREATED SUCCESSFULLY: {output_msi}")
print(f"Size: {os.path.getsize(output_msi) / (1024*1024):.2f} MB")
print("==========================================")
