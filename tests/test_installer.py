"""Offline installer regression tests; no user files, network or package changes."""
import hashlib
import importlib.util
import io
import json
import os
import select
import shlex
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('installer', ROOT / 'install-linux.py')
installer = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(installer)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.destination = self.root / 'bin'
        self.destination.mkdir()

    def archive(self, *, missing=None, symlink=None, duplicate=False, legacy=False):
        path = self.root / 'release.tar.gz'
        with tarfile.open(path, 'w:gz') as bundle:
            for source, _, _ in installer.FILES:
                if source == missing or (legacy and any('/' + m + '/' in source for m in ('local', 'icecast'))):
                    continue
                data = ('new:' + source).encode()
                member = tarfile.TarInfo(source)
                member.size = len(data)
                if source == symlink:
                    member.type = tarfile.SYMTYPE
                    member.linkname = '/etc/passwd'
                bundle.addfile(member, io.BytesIO(data) if source != symlink else None)
                if duplicate:
                    bundle.addfile(member, io.BytesIO(data))
                    duplicate = False
        return path

    def test_installs_files_and_preserves_custom_modules(self):
        custom = self.destination / 'modules/custom/notes'
        custom.parent.mkdir(parents=True)
        custom.write_text('keep')
        installer.install(self.archive(), self.destination)
        self.assertEqual(custom.read_text(), 'keep')
        for source, target, mode in installer.FILES:
            self.assertEqual((self.destination / target).read_text(), 'new:' + source)
            self.assertEqual((self.destination / target).stat().st_mode & 0o777, mode)

    def test_incomplete_archive_leaves_old_installation_intact(self):
        old = self.destination / 'mz'
        old.write_text('old')
        with self.assertRaises(KeyError):
            installer.install(self.archive(missing='musiczero/mz'), self.destination)
        self.assertEqual(old.read_text(), 'old')
        self.assertEqual(list(self.destination.iterdir()), [old])

    def test_failed_replacement_rolls_back_existing_and_new_files(self):
        for _, target, _ in installer.FILES[::2]:
            path = self.destination / target
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('old:' + target)
        before = {p.relative_to(self.destination): p.read_bytes()
                  for p in self.destination.rglob('*') if p.is_file()}
        replace = os.replace
        count = 0
        def failing_replace(source, target):
            nonlocal count
            count += 1
            if count == 5:
                raise OSError('disk failure')
            replace(source, target)
        with patch.object(installer.os, 'replace', side_effect=failing_replace):
            with self.assertRaisesRegex(OSError, 'disk failure'):
                installer.install(self.archive(), self.destination)
        after = {p.relative_to(self.destination): p.read_bytes()
                 for p in self.destination.rglob('*') if p.is_file()}
        self.assertEqual(after, before)

    def test_rejects_archive_symlink_before_changing_files(self):
        with self.assertRaisesRegex(RuntimeError, 'Invalid archive'):
            installer.install(self.archive(symlink='musiczero/mz'), self.destination)
        self.assertEqual(list(self.destination.iterdir()), [])

    def test_rejects_duplicate_archive_entries(self):
        with self.assertRaisesRegex(RuntimeError, 'Duplicate'):
            installer.install(self.archive(duplicate=True), self.destination)

    def test_rejects_existing_module_directory_symlink(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (self.destination / 'modules').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(RuntimeError, 'symlink'):
            installer.install(self.archive(), self.destination)
        self.assertEqual(list(outside.iterdir()), [])

    def test_legacy_archive_without_optional_modules_still_installs(self):
        installer.install(self.archive(legacy=True), self.destination)
        self.assertTrue((self.destination / 'mz').is_file())
        self.assertFalse((self.destination / 'modules/local').exists())

    def test_hash_mismatch_never_installs(self):
        archive = self.archive()
        args = type('Args', (), {'setup_only': False})()
        with patch.dict(os.environ, {'MUSICZERO_PREFIX': str(self.root)}), \
             patch.object(installer, 'release_asset', return_value=('test', 'unused', '0' * 64)), \
             patch.object(installer, 'open_url', side_effect=lambda url: archive.open('rb')):
            with self.assertRaisesRegex(RuntimeError, 'SHA-256'):
                installer.perform_install(args, None)
        self.assertFalse((self.destination / 'mz').exists())

    def test_noninteractive_download_and_install(self):
        archive = self.archive()
        digest = hashlib.sha256(archive.read_bytes()).hexdigest()
        args = type('Args', (), {'setup_only': False})()
        with patch.dict(os.environ, {'MUSICZERO_PREFIX': str(self.root)}), \
             patch.object(installer, 'release_asset', return_value=('test', 'unused', digest)), \
             patch.object(installer, 'open_url', side_effect=lambda url: archive.open('rb')):
            installer.perform_install(args, None)
        self.assertTrue((self.destination / 'mz').is_file())

    def test_path_line_handles_shell_metacharacters_and_is_idempotent(self):
        ui = type('UI', (), {'yes': lambda self, title: True})()
        directory = self.root / "some 'spaces $(false)"
        with patch.dict(os.environ, {'HOME': str(self.root), 'SHELL': '/bin/bash', 'PATH': '/usr/bin'}):
            installer.add_to_path(ui, directory)
            os.environ['PATH'] = '/usr/bin'
            installer.add_to_path(ui, directory)
            config = self.root / '.bashrc'
            self.assertEqual(config.read_text().count('# MusicZero'), 1)
            os.environ["PATH"] = "/usr/bin"
            result = subprocess.check_output(['/bin/bash', '-c',
                                              '. ' + shlex.quote(str(config)) + '; printf "%s" "$PATH"'],
                                             text=True)
            self.assertEqual(result, str(directory) + ':/usr/bin')

    def test_configure_retries_failed_setting_and_visits_all_sources(self):
        choices = iter([0, 0, 1, 2, 3, 4])  # local retry then remaining sources, finish
        class UI:
            def choose(self, title, options):
                return 0 if title.startswith('Setup failed') else next(choices)
            def text(self, title, default='', secret=False):
                return 'https://example.org/live' if 'URL' in title else '/music'
            def notice(self, title, message):
                pass
            def yes(self, title):
                return 'username/password' in title
        calls = []
        failed = False
        def command(args, capture=False):
            nonlocal failed
            calls.append(args[1:])
            if args[1] == 'modules':
                return '\n'.join(m + ' — source' for m in ('local', 'icecast', 'ymz', 'youmz'))
            if args[1:4] == ['set', 'local', 'path'] and not failed:
                failed = True
                raise RuntimeError('invalid path')
            return ''
        with patch.object(installer, 'dependencies', return_value=True), \
             patch.object(installer, 'run_command', side_effect=command):
            installer.configure(UI(), self.destination / 'mz')
        self.assertEqual(calls.count(['set', 'local', 'path', '/music']), 2)
        self.assertIn(['set', 'icecast', 'password', '/music'], calls)
        self.assertIn(['login', 'ymz'], calls)
        self.assertIn(['login', 'youmz'], calls)

    def test_missing_dependencies_require_a_choice_and_are_rechecked(self):
        ui = type('UI', (), {'yes': lambda self, title: True})()
        installed = False
        calls = []
        def which(name):
            if name in ('apt-get', 'sudo') or (installed and name in ('ffmpeg', 'ffprobe')):
                return '/usr/bin/' + name
            return None
        def run(args):
            nonlocal installed
            calls.append(args)
            if 'install' in args:
                installed = True
        with patch.object(installer.shutil, 'which', side_effect=which),              patch.object(installer.os, 'geteuid', return_value=1000),              patch.object(installer, 'run_command', side_effect=run):
            self.assertTrue(installer.dependencies(ui, 'local'))
        self.assertEqual(calls, [['sudo', 'apt-get', 'update'],
                                 ['sudo', 'apt-get', 'install', '-y', 'ffmpeg']])

    def test_declining_dependency_install_does_not_run_package_manager(self):
        ui = type('UI', (), {'yes': lambda self, title: False,
                             'choose': lambda self, title, options: 0})()
        with patch.object(installer.shutil, 'which', side_effect=lambda name: '/usr/bin/apt-get' if name == 'apt-get' else None),              patch.object(installer, 'run_command') as run:
            self.assertFalse(installer.dependencies(ui, 'icecast'))
            run.assert_not_called()

    def test_source_installer_respects_custom_cargo_target_directory(self):
        tools = self.root / "tools"
        tools.mkdir()
        cargo = tools / "cargo"
        cargo.write_text("#!/usr/bin/env python3\nimport pathlib,sys\n"
                         "target=pathlib.Path(sys.argv[sys.argv.index('--target-dir')+1])/'release'\n"
                         "target.mkdir(parents=True)\n"
                         "for name in ['mz','ymz-module','youmz-module','local-module','icecast-module']:\n"
                         "    (target/name).write_text('built in custom target')\n")
        cargo.chmod(0o755)
        env = dict(os.environ, PATH=str(tools) + os.pathsep + os.environ['PATH'],
                   CARGO_TARGET_DIR=str(self.root / 'custom build'), MUSICZERO_PREFIX=str(self.root / 'prefix'))
        subprocess.run([str(ROOT / 'install.sh')], env=env, check=True, stdout=subprocess.PIPE)
        bin_dir = self.root / 'prefix/bin'
        self.assertEqual((bin_dir / 'mz').read_text(), 'built in custom target')
        for module in ('local', 'icecast', 'ymz', 'youmz'):
            self.assertTrue((bin_dir / 'modules' / module / 'module.json').is_file())
            self.assertEqual((bin_dir / 'modules' / module / (module + '-module')).read_text(),
                             'built in custom target')

    def test_station_password_mask_is_never_submitted_as_a_secret(self):
        choices = iter([2, 0, 14])  # url alias, station selector, Back
        settings = {"settings": {"station": "one", "buffer_ms": 1000,
                    "stations": [{"id": "one", "name": "Radio", "url": "https://example.org",
                                  "username": "user", "password": "***"}]}}
        calls = []
        class UI:
            def choose(self, title, options):
                # Choose Back after editing a URL and changing the selected station.
                selected = next(choices)
                return len(options) - 1 if selected == 14 else selected
            def text(self, title, default='', secret=False):
                return default
        def command(args, capture=False):
            calls.append(args[1:])
            return json.dumps(settings)
        with patch.object(installer, 'run_command', side_effect=command):
            installer.edit_settings(UI(), self.destination / 'mz', 'icecast')
        changes = [args for args in calls if args[0] == 'set']
        self.assertEqual(changes, [['set', 'icecast', 'url', 'https://example.org'],
                                   ['set', 'icecast', 'station', 'one']])
        self.assertFalse(any('***' in args or 'stations' in args for args in changes))

    def test_legacy_cli_flat_settings_are_edited_without_protocol_wrapper(self):
        choices = iter([0, 1])
        class UI:
            def choose(self, title, options):
                return next(choices)
            def text(self, title, default='', secret=False):
                return '/new music'
        calls = []
        def command(args, capture=False):
            calls.append(args[1:])
            return 'old help' if args[1] == 'help' else '{"path":"/old music"}'
        with patch.object(installer, 'run_command', side_effect=command):
            installer.edit_settings(UI(), self.destination / 'mz', 'local')
        self.assertIn(['set', 'local', 'path', '/new music'], calls)

    def test_native_settings_menu_bypasses_legacy_json_editor(self):
        calls = []
        def command(args, capture=False):
            calls.append(args[1:])
            return 'mz config [module]' if args[1] == 'help' else ''
        with patch.object(installer, 'run_command', side_effect=command):
            installer.edit_settings(None, self.destination / 'mz', 'ymz')
        self.assertEqual(calls, [['help'], ['config', 'ymz']])

    def test_invalid_settings_schema_returns_a_readable_error(self):
        with patch.object(installer, 'run_command', side_effect=['old help', '[]']):
            with self.assertRaisesRegex(RuntimeError, 'JSON object'):
                installer.edit_settings(None, self.destination / 'mz', 'local')

    @unittest.skipUnless(sys.platform == 'linux', 'Linux controlling terminal')
    def test_piped_python_tui_login_reads_controlling_terminal(self):
        import fcntl
        import pty
        import struct
        import termios
        executable = self.destination / 'mz'
        log = self.root / 'login'
        executable.write_text('#!/bin/sh\ncase "$1" in\n'
                              'modules) echo "ymz — Yandex";;\n'
                              'login) printf "LOGIN_INPUT: "; read answer; '
                              'printf "%s" "$answer" > ' + shlex.quote(str(log)) + ';;\nesac\n')
        executable.chmod(0o755)
        master, slave = pty.openpty()
        self.addCleanup(os.close, master)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 100, 0, 0))
        def session():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        env = dict(os.environ, TERM='xterm', MUSICZERO_PREFIX=str(self.root), SHELL='/bin/bash',
                   PATH=str(self.destination) + os.pathsep + os.environ['PATH'])
        process = subprocess.Popen([sys.executable, '-', '--tui', '--setup-only'],
                                   stdin=subprocess.PIPE, stdout=slave, stderr=slave,
                                   pass_fds=(slave,), preexec_fn=session, env=env)
        os.close(slave)
        self.addCleanup(lambda: process.poll() is None and process.kill())
        process.stdin.write((ROOT / 'install-linux.py').read_bytes())
        process.stdin.close()
        output = bytearray()
        def expect(marker):
            end = time.monotonic() + 10
            while marker not in output:
                if time.monotonic() > end:
                    self.fail('Terminal output missing ' + repr(marker) + ': ' + repr(bytes(output)))
                ready, _, _ = select.select([master], [], [], 0.1)
                if ready:
                    try:
                        chunk = os.read(master, 65536)
                    except OSError:
                        self.fail('PTY closed: ' + repr(bytes(output)))
                    output.extend(chunk)
            output.clear()
        expect(b'Configure sources')
        os.write(master, b'\n')
        expect(b'LOGIN_INPUT:')
        os.write(master, b'terminal-token\n')
        expect(b'Edit advanced ymz')
        os.write(master, b'\n')
        expect(b'Start ymz')
        os.write(master, b'\n')  # no playback
        expect(b'Configure sources')
        os.write(master, b'j\n')  # finish
        self.assertEqual(process.wait(timeout=10), 0)
        self.assertEqual(log.read_text(), 'terminal-token')


if __name__ == '__main__':
    unittest.main()
