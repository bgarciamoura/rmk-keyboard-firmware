"""
Monitor serial com retry agressivo. Útil quando o chip está em boot loop
e a porta CDC aparece/some ciclicamente (ex.: ESP32-S3 USB-Serial/JTAG
após flash de firmware que panica).

Estratégia: tenta abrir a porta em loop até conseguir, lê continuamente,
e reabre automaticamente se a porta cair. Pressionar Ctrl+C encerra.

Usage: python monitor.py [PORT] [BAUD]
Default: COM13 @ 115200
"""
import serial
import sys
import time

port = sys.argv[1] if len(sys.argv) > 1 else "COM13"
baud = int(sys.argv[2]) if len(sys.argv) > 2 else 115200

print(f"[monitor on {port} @ {baud} — Ctrl+C para sair]")
print(f"[se chip está em boot loop: replugue USB agora]\n")

try:
    while True:
        # Loop externo: reabrir porta se cair
        s = None
        attempts = 0
        while s is None:
            try:
                s = serial.Serial(port, baud, timeout=0.1, dsrdtr=False, rtscts=False)
                s.dtr = False
                s.rts = False
                print(f"\n[porta aberta após {attempts} tentativas]\n", file=sys.stderr)
            except (serial.SerialException, OSError):
                attempts += 1
                if attempts % 50 == 0:
                    print(f"[aguardando porta... {attempts} tentativas]", file=sys.stderr)
                time.sleep(0.02)

        # Loop interno: ler até a porta cair
        try:
            while True:
                data = s.read(512)
                if data:
                    sys.stdout.write(data.decode(errors="replace"))
                    sys.stdout.flush()
        except (serial.SerialException, OSError) as e:
            print(f"\n[porta caiu: {e} — reabrindo]", file=sys.stderr)
            try:
                s.close()
            except Exception:
                pass
except KeyboardInterrupt:
    print("\n[done]")
