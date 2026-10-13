@echo off
setlocal
cd /d "%~dp0"
if exist ".tools\cargo\bin\cargo.exe" (
  set "CARGO_HOME=%CD%\.tools\cargo"
  set "RUSTUP_HOME=%CD%\.tools\rustup"
  set "PATH=%CD%\.tools\cargo\bin;%PATH%"
)
if exist ".tools\msvc\setup_x64.bat" call ".tools\msvc\setup_x64.bat"
where rc.exe >nul 2>nul
if errorlevel 1 (
  for /f "usebackq tokens=*" %%i in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do call "%%i\VC\Auxiliary\Build\vcvars64.bat"
)
set "CARGO_INCREMENTAL=0"
cargo build --release --locked --target x86_64-pc-windows-msvc
if errorlevel 1 exit /b 1
copy /y "target\x86_64-pc-windows-msvc\release\epub-reader.exe" "epub-reader.exe" >nul
if errorlevel 1 exit /b 1
echo Built: %CD%\epub-reader.exe
for %%i in (epub-reader.exe) do echo Size: %%~zi bytes
endlocal
