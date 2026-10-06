"""Inspect every Android alpha executable, including ABI and load-time dependencies."""
import os
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parents[1]
BIN = pathlib.Path(os.environ.get('MZ_ANDROID_BIN', ROOT / 'target/aarch64-linux-android/release'))
ALLOWED = {'libc.so', 'libm.so', 'libdl.so', 'liblog.so'}

for name in ['mz', 'ymz-module', 'youmz-module', 'local-module', 'icecast-module']:
    binary = BIN / name
    header = subprocess.check_output(['readelf', '-h', str(binary)], text=True)
    program = subprocess.check_output(['readelf', '-l', str(binary)], text=True)
    dynamic = subprocess.check_output(['readelf', '-d', str(binary)], text=True)
    assert 'AArch64' in header and 'ELF64' in header, header
    assert '/system/bin/linker64' in program, program
    needed = {line.split('[')[1].split(']')[0] for line in dynamic.splitlines() if '(NEEDED)' in line}
    assert needed <= ALLOWED, (name, needed)
    assert 'GLIBC_' not in subprocess.check_output(['readelf', '-V', str(binary)], text=True)
    print(f'Android ARM64: {name}: native Bionic executable; dependencies {sorted(needed)}')
