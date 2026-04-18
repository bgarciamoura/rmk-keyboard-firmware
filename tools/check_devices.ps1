# check_devices.ps1
# Lista TODOS os dispositivos USB com VID 4C4B e 303A, mostrando manufacturer,
# localização física (bus/porta) e data de instalação. Útil pra diferenciar
# dongle recém-flasheado de keyboard antigo que compartilha o mesmo VID/PID.

function Show-Device {
    param($Device)
    $props = Get-PnpDeviceProperty -InstanceId $Device.InstanceId -ErrorAction SilentlyContinue
    $location     = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_LocationInfo').Data
    $manufacturer = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_Manufacturer').Data
    $parent       = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_Parent').Data
    $busReported  = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_BusReportedDeviceDesc').Data
    $installDate  = ($props | Where-Object KeyName -eq 'DEVPKEY_Device_InstallDate').Data

    Write-Host "  FriendlyName : $($Device.FriendlyName)" -ForegroundColor Yellow
    Write-Host "  InstanceId   : $($Device.InstanceId)"
    Write-Host "  Class        : $($Device.Class)"
    Write-Host "  Status       : $($Device.Status)"
    if ($manufacturer) { Write-Host "  Manufacturer : $manufacturer" }
    if ($busReported)  { Write-Host "  BusReported  : $busReported" }
    if ($location)     { Write-Host "  Location     : $location" }
    if ($installDate)  { Write-Host "  InstallDate  : $installDate" }
    if ($parent)       { Write-Host "  Parent       : $parent" }
    Write-Host ""
}

Write-Host "=============================================================" -ForegroundColor Cyan
Write-Host "Timestamp: $(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')"
Write-Host ""

Write-Host "=== USB composite/root devices com VID 4C4B ===" -ForegroundColor Cyan
$usb4c4b = Get-PnpDevice -PresentOnly | Where-Object {
    $_.InstanceId -match '^USB\\VID_4C4B' -and $_.InstanceId -notmatch '&MI_'
}
if ($usb4c4b.Count -eq 0) {
    Write-Host "  (nenhum)" -ForegroundColor DarkGray
} else {
    Write-Host "  Total: $($usb4c4b.Count) dispositivo(s) físico(s) encontrado(s)" -ForegroundColor Green
    Write-Host ""
    foreach ($d in $usb4c4b) { Show-Device $d }
}

Write-Host "=== USB devices com VID 303A (Espressif) ===" -ForegroundColor Cyan
$usb303a = Get-PnpDevice -PresentOnly | Where-Object {
    $_.InstanceId -match '^USB\\VID_303A' -and $_.InstanceId -notmatch '&MI_'
}
if ($usb303a.Count -eq 0) {
    Write-Host "  (nenhum)" -ForegroundColor DarkGray
} else {
    Write-Host "  Total: $($usb303a.Count)" -ForegroundColor Green
    Write-Host ""
    foreach ($d in $usb303a) { Show-Device $d }
}

Write-Host "=== Interfaces HID de VID 4C4B (detalhe) ===" -ForegroundColor Cyan
Get-PnpDevice -PresentOnly |
    Where-Object { $_.InstanceId -match '^HID\\VID_4C4B' } |
    ForEach-Object {
        Write-Host "  $($_.InstanceId.PadRight(70)) | $($_.FriendlyName)"
    }
