@echo off
rem Builds lua5.1.dll with the MSVC Rust toolchain (x86_64-pc-windows-msvc).
rem Requirements: Rust >= 1.92 (rustup default stable-msvc) and the
rem "Desktop development with C++" / Build Tools (for cl.exe and link.exe).
setlocal
cargo build --release || exit /b 1
if not exist dist mkdir dist
if not exist dist\include mkdir dist\include
copy /Y target\release\lua51.dll dist\lua5.1.dll >nul || exit /b 1
if exist target\release\lua51.pdb copy /Y target\release\lua51.pdb dist\lua5.1.pdb >nul
rem Import libraries only encode the DLL name and the export names, so the
rem prebuilt ones in lib\ match any build of lua5.1.dll. To regenerate:
rem   lib /def:lua5.1.def /out:dist\lua5.1.lib /machine:x64
copy /Y lib\lua5.1.lib dist\lua5.1.lib >nul
copy /Y lib\liblua5.1.dll.a dist\liblua5.1.dll.a >nul
copy /Y lua5.1.def dist\lua5.1.def >nul
copy /Y include\*.h dist\include\ >nul
echo.
echo Built dist\lua5.1.dll (+ lua5.1.lib import library, headers in dist\include)
