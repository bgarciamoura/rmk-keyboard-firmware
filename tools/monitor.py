"""
Monitor serial simples sem auto-reset. Apenas abre a porta e lê por 30s.
Para resetar: pressione o botão RESET da placa OU desplugue/replugue o USB.

Usage: python monitor.py [PORT] [BAUD]
Default: COM13 @ 115200
"""
import serial
import sys
import time

port = sys.argv[1] if len(sys.argv) > 1 else "COM13"
baud = int(sys.argv[2]) if len(sys.argv) > 2 else 115200

print(f"[opening {port} @ {baud}]")
# dsrdtr=False + rtscts=False impede que o pyserial toggle DTR/RTS ao abrir,
# o que no ESP32-S3 native USB-JTAG estava triggerando download mode.
s = serial.Serial(port, baud, timeout=0.2, dsrdtr=False, rtscts=False)
# Garantir DTR/RTS em repouso
s.dtr = False
s.rts = False

print("[reading 30s — pressione RESET na placa ou replugue USB para bootar]\n")
end = time.time() + 30
while time.time() < end:
    data = s.read(512)
    if data:
        sys.stdout.write(data.decode(errors="replace"))
        sys.stdout.flush()

s.close()
print("\n[done]")
