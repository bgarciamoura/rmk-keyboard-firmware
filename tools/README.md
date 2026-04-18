# tools/

Scripts de diagnóstico utilizados durante a migração do F1 do dongle ESP32-S3.
Não fazem parte do build do firmware — são apenas para verificação de hardware e
USB/serial no Windows (onde o ecossistema ESP32 é menos polido).

| Script | Propósito |
|--|--|
| `monitor.py` | Abre uma porta serial sem forçar reset (diferente do `miniterm`, que na versão atual do pyserial no Python 3.13 tem bug + dispara download mode no ESP32-S3). Default `COM13 @ 115200`. Usage: `python tools/monitor.py [PORT] [BAUD]` |
| `list_ports.py` | Lista todas as portas seriais presentes e procura por VIDs conhecidos (4C4B = nossa HID, 303A = Espressif). Usage: `python tools/list_ports.py` |
| `check_devices.ps1` | PowerShell script que lista dispositivos USB com VID 4C4B / 303A, incluindo manufacturer, localização física e data de instalação. Útil para diferenciar dongle recém-flasheado de periféricos antigos que compartilham o mesmo VID/PID. Usage: `powershell -File tools/check_devices.ps1` |

Em geral, só precisas destes se o firmware parecer não bootar, não enumerar como HID, ou houver confusão sobre qual dispositivo está conectado em qual porta USB.
