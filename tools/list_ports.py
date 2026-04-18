"""
Lista todos os dispositivos USB/serial conectados com VID:PID.
Útil para descobrir onde o ESP32-S3 reenumerou após um reset.
"""
import serial.tools.list_ports

print("=== Portas COM/serial disponíveis ===")
for p in sorted(serial.tools.list_ports.comports(), key=lambda x: x.device):
    print(f"  {p.device:10} | {p.description}")
    print(f"  {'':10} | hwid: {p.hwid}")
    print()

# Procurar especificamente VID 4C4B / PID 4643 (Charybdis) ou 303A (Espressif)
targets = {"4C4B:4643": "Charybdis 3x6 Wireless (RMK HID)", "303A": "Espressif (ESP32-S3)"}
print("=== Dispositivos de interesse ===")
found = False
for p in serial.tools.list_ports.comports():
    hwid = p.hwid.upper() if p.hwid else ""
    for key, label in targets.items():
        if key in hwid:
            print(f"  MATCH {label}: {p.device} — {p.description}")
            found = True
if not found:
    print("  (nenhum dispositivo com VID/PID conhecido encontrado)")
    print("  Lista todos os HID manualmente no Device Manager.")
