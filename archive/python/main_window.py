# -*- coding: utf-8 -*-
"""Fenêtre principale de Compress Speak."""

import sys
import subprocess
import tempfile
import shutil
import time
from pathlib import Path

from PyQt6 import QtCore, QtGui, QtWidgets, QtMultimedia

import i18n
from config import (
    FFMPEG_CMD,
    DEFAULT_AFFTDN_NF,
    DEFAULT_COMP_ATTACK_MS,
    DEFAULT_COMP_RELEASE_MS,
    DEFAULT_COMP_RATIO,
    DEFAULT_COMP_THRESHOLD_DB,
    DEFAULT_GAIN_DB,
    DEFAULT_OPUS_BITRATE,
    DEFAULT_OPUS_PREVIEW_BITRATE,
    MAX_GAIN_DB,
    MIN_GAIN_DB,
    SETTINGS_AFFTDN_NF,
    SETTINGS_COMP_ATTACK,
    SETTINGS_COMP_RELEASE,
    SETTINGS_COMP_RATIO,
    SETTINGS_COMP_THRESHOLD,
    SETTINGS_OPUS_BITRATE,
    SETTINGS_OPUS_PREVIEW_BITRATE,
    SETTINGS_UVR5_CHUNK_SEC,
    SETTINGS_UVR5_DIRECTML_DEVICE,
    SETTINGS_UVR5_ENABLED,
    SETTINGS_UVR5_MDX_BATCH_SIZE,
    SETTINGS_UVR5_MDX_SEGMENT_SIZE,
    SETTINGS_UVR5_MDXC_OVERLAP,
    SETTINGS_UVR5_MODEL,
    SETTINGS_UVR5_PATH,
    SETTINGS_UVR5_STEM,
    SETTINGS_VIBE_FORMAT,
    SETTINGS_VIBE_LANGUAGE,
    SETTINGS_VIBE_MODEL,
    SETTINGS_VIBE_PATH,
    SETTINGS_FORM_CLEAN_SUFFIX,
    SETTINGS_FORM_KEEP_FLAC,
    SETTINGS_OUTPUT_DIR,
    SETTINGS_WARMTH_COMPRESSOR_RATIO,
    SETTINGS_WARMTH_COMPRESSOR_THRESHOLD,
    SETTINGS_WARMTH_DISTORTION_DRIVE,
    SETTINGS_WARMTH_FINAL_GAIN,
    SETTINGS_WARMTH_HIGH_SHELF_CUTOFF,
    SETTINGS_WARMTH_HIGH_SHELF_GAIN,
    DEFAULT_WARMTH_COMPRESSOR_RATIO,
    DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB,
    DEFAULT_WARMTH_DISTORTION_DRIVE_DB,
    DEFAULT_WARMTH_FINAL_GAIN_DB,
    DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ,
    DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB,
    SETTINGS_DPDF_MODEL,
    SETTINGS_DPDF_ATTN_LIMIT_DB,
    DEFAULT_DPDF_MODEL,
    DEFAULT_DPDF_ATTN_LIMIT_DB,
    VIBE_CMD,
    VIBE_FORMATS,
    VIBE_LANGUAGES,
)
from widgets import AudioPlayer, CircularProgressWidget, DropContainer
from theme import ThemeManager
import vibe
import ffmpeg
import uvr5 as uvr5_module
import warmth
import dpdf


class Uvr5InProcessWorker(QtCore.QObject):
    """Exécute run_uvr5_vocals_inprocess (API + DirectML) dans un thread et émet log_line / result_ready."""

    log_line = QtCore.pyqtSignal(str)
    result_ready = QtCore.pyqtSignal(object)  # Path | None

    def run(
        self,
        input_path: Path,
        output_dir: Path,
        chunk_sec: int,
        model: str,
        mdx_seg: int,
        mdxc_ov: int,
        mdx_bs: int,
    ):
        def log_cb(text: str):
            self.log_line.emit(text)

        vocals_path = uvr5_module.run_uvr5_vocals_inprocess(
            input_path,
            output_dir,
            chunk_sec,
            model=model,
            mdx_segment_size=mdx_seg,
            mdxc_overlap=mdxc_ov,
            mdx_batch_size=mdx_bs,
            log_callback=log_cb,
        )
        self.result_ready.emit(vocals_path)


class MainWindow(QtWidgets.QWidget):
    def __init__(self, theme_manager: ThemeManager, parent=None):
        super().__init__(parent)
        self.theme_manager = theme_manager
        self.setMinimumSize(640, 480)
        self.current_file: Path | None = None
        
        # Initialiser les caches Vibe/sona tôt (avant _retranslate_ui)
        self._vibe_available_cached: bool | None = None
        self._vibe_check_scheduled: bool = False
        self._vibe_sona_syntax_cached: bool | None = None
        self._vibe_path_cached: str | None = None
        self._sona_auto_detected_logged: bool = False
        
        # Cache pour UVR5
        self._uvr5_available_cached: bool | None = None

        main_layout = QtWidgets.QVBoxLayout(self)

        self.drop_container = DropContainer()
        drop_layout = QtWidgets.QVBoxLayout(self.drop_container)
        drop_layout.setContentsMargins(4, 4, 4, 4)
        self._queue_label = QtWidgets.QLabel()
        drop_layout.addWidget(self._queue_label)
        self.queue_table = QtWidgets.QTableWidget()
        self.queue_table.setColumnCount(8)
        self.queue_table.setHorizontalHeaderLabels(
            [
                "",
                i18n.tr("queue.header_file"),
                i18n.tr("queue.header_gain"),
                i18n.tr("queue.header_isolate"),
                i18n.tr("queue.header_transcribe"),
                i18n.tr("queue.header_warmth"),
                i18n.tr("queue.header_dpdf"),
                "",
            ]
        )
        self.queue_table.horizontalHeader().setSectionResizeMode(1, QtWidgets.QHeaderView.ResizeMode.Stretch)
        self.queue_table.setColumnWidth(0, 40)
        self.queue_table.setColumnWidth(3, 90)
        self.queue_table.setColumnWidth(4, 90)
        self.queue_table.setColumnWidth(5, 70)
        self.queue_table.setColumnWidth(6, 70)
        self.queue_table.setSelectionBehavior(QtWidgets.QAbstractItemView.SelectionBehavior.SelectRows)
        self.queue_table.setSelectionMode(QtWidgets.QAbstractItemView.SelectionMode.SingleSelection)
        self.queue_table.setMinimumHeight(100)
        drop_layout.addWidget(self.queue_table)
        self.add_files_button = QtWidgets.QPushButton(i18n.tr("btn.add_files"))
        drop_layout.addWidget(self.add_files_button)
        self.drop_container.register_child(self.queue_table)
        self.drop_container.register_child(self.queue_table.viewport())
        self.drop_container.register_child(self.add_files_button)
        main_layout.addWidget(self.drop_container)

        self.player = AudioPlayer()
        main_layout.addWidget(self.player)
        self.player.media_player.durationChanged.connect(self._update_duration)
        self.player.preview_button.clicked.connect(self._preview_segment)
        self.player.return_button.clicked.connect(self._restore_original_file)
        self.player.media_player.mediaStatusChanged.connect(self._handle_media_status)

        form_layout = QtWidgets.QFormLayout()
        suffix_row = QtWidgets.QHBoxLayout()
        self.suffix_edit = QtWidgets.QLineEdit()
        self.suffix_edit.setPlaceholderText(i18n.tr("form.suffix_placeholder"))
        self.suffix_edit.setMaximumWidth(180)
        self.suffix_edit.setToolTip(i18n.tr("form.tooltip_suffix"))
        suffix_row.addWidget(self.suffix_edit)
        self.keep_flac_checkbox = QtWidgets.QCheckBox(i18n.tr("form.keep_flac"))
        self.keep_flac_checkbox.setChecked(False)
        self.keep_flac_checkbox.setToolTip(i18n.tr("form.tooltip_keep_flac"))
        suffix_row.addWidget(self.keep_flac_checkbox)
        suffix_row.addStretch()
        self._suffix_label = QtWidgets.QLabel(i18n.tr("form.suffix_label"))
        form_layout.addRow(self._suffix_label, suffix_row)

        self.output_dir_edit = QtWidgets.QLineEdit()
        self.output_dir_edit.setPlaceholderText(i18n.tr("form.output_placeholder"))
        self._output_dir_label = QtWidgets.QLabel(i18n.tr("form.output_dir"))
        form_layout.addRow(self._output_dir_label, self.output_dir_edit)
        main_layout.addLayout(form_layout)

        action_layout = QtWidgets.QHBoxLayout()
        self.convert_button = QtWidgets.QPushButton(i18n.tr("btn.convert"))
        self.cancel_button = QtWidgets.QPushButton(i18n.tr("btn.cancel"))
        self.cancel_button.setEnabled(False)
        self.open_folder_button = QtWidgets.QPushButton(i18n.tr("btn.choose"))
        action_layout.addWidget(self.open_folder_button)
        action_layout.addWidget(self.cancel_button)
        action_layout.addStretch()
        self.transcribe_button = QtWidgets.QPushButton(i18n.tr("btn.transcribe"))
        self.transcribe_button.setToolTip(i18n.tr("transcribe.tooltip"))
        self.transcribe_button.setEnabled(False)
        action_layout.addWidget(self.transcribe_button)
        self.convert_button.setToolTip(i18n.tr("btn.convert"))
        action_layout.addWidget(self.convert_button)
        self.process_all_button = QtWidgets.QPushButton(i18n.tr("btn.process_all"))
        self.process_all_button.setToolTip(i18n.tr("process_all.tooltip"))
        self.process_all_button.setEnabled(False)
        action_layout.addWidget(self.process_all_button)
        self._settings_button = QtWidgets.QToolButton()
        self._settings_button.setText("⚙")
        self._settings_button.setPopupMode(QtWidgets.QToolButton.ToolButtonPopupMode.InstantPopup)
        self._settings_menu = QtWidgets.QMenu(self)
        self._settings_menu.aboutToShow.connect(self._apply_palette_to_menu)
        self._action_theme_light = self._settings_menu.addAction(i18n.tr("theme.light"))
        self._action_theme_dark = self._settings_menu.addAction(i18n.tr("theme.dark"))
        self._action_theme_light.triggered.connect(lambda: self._set_theme("light"))
        self._action_theme_dark.triggered.connect(lambda: self._set_theme("dark"))
        self._settings_menu.addSeparator()
        self._action_lang_fr = self._settings_menu.addAction(i18n.tr("lang.fr"))
        self._action_lang_en = self._settings_menu.addAction(i18n.tr("lang.en"))
        self._action_lang_de = self._settings_menu.addAction(i18n.tr("lang.de"))
        self._action_lang_es = self._settings_menu.addAction(i18n.tr("lang.es"))
        for act, lang in (
            (self._action_lang_fr, "fr"),
            (self._action_lang_en, "en"),
            (self._action_lang_de, "de"),
            (self._action_lang_es, "es"),
        ):
            act.setCheckable(True)
            act.triggered.connect(lambda checked, l=lang: self._set_language(l))
        self._settings_menu.addSeparator()
        self._action_vibe_path = self._settings_menu.addAction(i18n.tr("vibe.path_menu"))
        self._action_vibe_path.triggered.connect(self._show_vibe_path_dialog)
        self._action_vibe_options = self._settings_menu.addAction(i18n.tr("vibe.options_menu"))
        self._action_vibe_options.triggered.connect(self._show_vibe_options_dialog)
        self._settings_menu.addSeparator()
        self._action_conv = self._settings_menu.addAction(i18n.tr("conv.menu"))
        self._action_conv.triggered.connect(self._show_conversion_settings_dialog)
        self._action_uvr5 = self._settings_menu.addAction(i18n.tr("uvr5.menu"))
        self._action_uvr5.triggered.connect(self._show_uvr5_dialog)
        self._action_warmth = self._settings_menu.addAction(i18n.tr("warmth.menu"))
        self._action_warmth.triggered.connect(self._show_warmth_dialog)
        self._action_dpdf = self._settings_menu.addAction(i18n.tr("dpdf.menu"))
        self._action_dpdf.triggered.connect(self._show_dpdf_dialog)
        self._settings_button.setMenu(self._settings_menu)
        action_layout.addWidget(self._settings_button)
        main_layout.addLayout(action_layout)

        self.progress_bar = QtWidgets.QProgressBar()
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.progress_bar.setTextVisible(True)
        self.progress_bar.setFormat(i18n.tr("progress.waiting"))
        self.progress_bar.setVisible(False)
        main_layout.addWidget(self.progress_bar)

        self.log_output = QtWidgets.QPlainTextEdit()
        self.log_output.setReadOnly(True)
        self.log_output.setMaximumBlockCount(1000)
        main_layout.addWidget(self.log_output, stretch=1)

        self.drop_container.files_dropped.connect(self._add_files_to_queue)
        self.add_files_button.clicked.connect(self._pick_files_to_add)
        self.queue_table.itemSelectionChanged.connect(self._on_queue_selection_changed)
        self.output_dir_edit.textChanged.connect(self._remember_output_dir)
        self.convert_button.clicked.connect(self._convert_audio)
        self.transcribe_button.clicked.connect(self._transcribe_current)
        self.process_all_button.clicked.connect(self._process_all)
        self.open_folder_button.clicked.connect(self._pick_output_dir)
        self.cancel_button.clicked.connect(self._cancel_conversion)
        self._refresh_theme_menu_actions()
        self._refresh_language_menu_actions()
        self._retranslate_ui()

        self._output_dir = None
        self._ffmpeg_process: QtCore.QProcess | None = None
        self._current_duration_sec: float | None = None
        self._cancel_requested = False
        self._original_media_path: Path | None = None

        self._preview_dir = Path(tempfile.mkdtemp(prefix="compressspeak_preview_"))
        self._preview_path: Path | None = None
        self._preview_process: QtCore.QProcess | None = None
        self._preview_return_position_ms: int = 0
        self._preview_snippet_duration_ms: int = 5000
        self._preview_was_playing: bool = False
        self._using_preview: bool = False
        self._pending_restore_position: int | None = None
        self._pending_restore_resume: bool = False
        self._conversion_queue: list[tuple[Path, int, bool, bool, bool, bool]] = []
        self._current_conversion_index: int = -1
        self._vibe_process: QtCore.QProcess | None = None
        self._transcribe_output_path: Path | None = None
        self._transcribe_queue: list[tuple[Path, Path]] = []
        self._current_transcribe_index: int = -1
        self._vibe_stdout_file: object = None  # fichier ouvert pour capture stdout (mode sona)
        self._process_all_mode: bool = False  # True si on est en mode "Tout traiter"
        self._process_all_transcribe_after: bool = False  # True si on doit transcrire après conversion
        self._uvr5_process: QtCore.QProcess | None = None
        self._uvr5_output_dir: Path | None = None
        self._conversion_after_uvr5_output_path: Path | None = None
        self._conversion_after_uvr5_gain: int = DEFAULT_GAIN_DB
        self._conversion_keep_flac: bool = False  # Conserver le FLAC après conversion
        self._conversion_apply_warmth: bool = False  # Appliquer la chaleur vocale
        self._conversion_apply_dpdf: bool = False  # Appliquer le nettoyage DPDF
        self._conversion_dpdf_applied: bool = False  # DPDF a réellement produit un fichier
        self._dpdf_temp_dir: Path | None = None  # Temp DPDF hors UVR5 (nettoyé après FFmpeg)
        self._uvr5_last_log_time: float = 0.0  # Pour throttling des logs UVR5
        self._uvr5_worker_vocals_path: Path | None = None  # Pour parser VOCALS_OUTPUT du worker
        self._uvr5_heartbeat_timer: QtCore.QTimer | None = None  # Timer pour vérifier que le processus n'est pas bloqué
        # Pré-conversion m4a/mp4 → WAV avant UVR5 (contourne un bug audio-separator avec m4a et fichiers temp sans extension)
        self._uvr5_prep_ffmpeg_process: QtCore.QProcess | None = None
        self._uvr5_cmd_after_prep: list[str] | None = None  # Commande worker/CLI à lancer après la conversion m4a→wav

        self.player.set_gain_db(DEFAULT_GAIN_DB)
        self._load_form_settings()
        self._check_uvr5_availability()
        self._update_transcribe_button()

    def _add_files_to_queue(self, paths: list[Path]):
        for path in paths:
            self._add_queue_row(path, DEFAULT_GAIN_DB)
        if paths:
            if not self.current_file:
                self.queue_table.selectRow(0)
                # S'assurer que le fichier est bien chargé (au cas où le signal ne se déclenche pas)
                self._load_file_from_queue(0)

    def _add_queue_row(self, path: Path, gain_db: int):
        row = self.queue_table.rowCount()
        self.queue_table.insertRow(row)
        progress_widget = CircularProgressWidget()
        self.queue_table.setCellWidget(row, 0, progress_widget)
        name_item = QtWidgets.QTableWidgetItem(path.name)
        name_item.setData(QtCore.Qt.ItemDataRole.UserRole, str(path))
        name_item.setFlags(name_item.flags() & ~QtCore.Qt.ItemFlag.ItemIsEditable)
        self.queue_table.setItem(row, 1, name_item)

        gain_spin = QtWidgets.QSpinBox()
        gain_spin.setRange(MIN_GAIN_DB, MAX_GAIN_DB)
        gain_spin.setValue(gain_db)
        gain_spin.setSuffix(" dB")
        gain_spin.valueChanged.connect(lambda v, r=row: self._queue_gain_changed(r, v))
        self.queue_table.setCellWidget(row, 2, gain_spin)

        isolate_cb = QtWidgets.QCheckBox()
        isolate_cb.setToolTip(i18n.tr("isolate.tooltip"))
        isolate_cb.setEnabled(getattr(self, "_uvr5_available_cached", None) is True)
        self.queue_table.setCellWidget(row, 3, isolate_cb)

        transcribe_cb = QtWidgets.QCheckBox()
        transcribe_cb.setToolTip(i18n.tr("transcribe.tooltip"))
        transcribe_cb.setEnabled(getattr(self, "_vibe_available_cached", None) is True)
        self.queue_table.setCellWidget(row, 4, transcribe_cb)

        warmth_cb = QtWidgets.QCheckBox()
        warmth_cb.setToolTip(i18n.tr("warmth.tooltip"))
        warmth_cb.setEnabled(warmth.is_pedalboard_available())
        self.queue_table.setCellWidget(row, 5, warmth_cb)

        dpdf_cb = QtWidgets.QCheckBox()
        dpdf_cb.setToolTip(i18n.tr("dpdf.tooltip"))
        dpdf_cb.setEnabled(dpdf.is_dpdf_available())
        self.queue_table.setCellWidget(row, 6, dpdf_cb)

        remove_btn = QtWidgets.QPushButton(i18n.tr("btn.remove"))
        remove_btn.setMaximumWidth(32)
        remove_btn.clicked.connect(lambda checked, r=row: self._remove_queue_row(r))
        self.queue_table.setCellWidget(row, 7, remove_btn)
        self.log_output.appendPlainText(i18n.tr("log.added", name=path.name))

    def _set_row_progress(self, row: int, value: int):
        if 0 <= row < self.queue_table.rowCount():
            w = self.queue_table.cellWidget(row, 0)
            if isinstance(w, CircularProgressWidget):
                w.set_progress(value)

    def _get_row_progress_widget(self, row: int) -> CircularProgressWidget | None:
        if 0 <= row < self.queue_table.rowCount():
            w = self.queue_table.cellWidget(row, 0)
            if isinstance(w, CircularProgressWidget):
                return w
        return None

    def _remove_queue_row(self, row: int):
        if row < 0 or row >= self.queue_table.rowCount():
            return
        was_selected = self.current_file and row == self._get_selected_row()
        self.queue_table.removeRow(row)
        if was_selected:
            self.current_file = None
            next_row = min(row, self.queue_table.rowCount() - 1)
            if next_row >= 0:
                self.queue_table.selectRow(next_row)
            else:
                self.player.clear_media()
            self._update_transcribe_button()
        self._renumber_lambda_rows()

    def _renumber_lambda_rows(self):
        for row in range(self.queue_table.rowCount()):
            gain_spin = self.queue_table.cellWidget(row, 2)
            if isinstance(gain_spin, QtWidgets.QSpinBox):
                try:
                    gain_spin.valueChanged.disconnect()
                except TypeError:
                    pass
                gain_spin.valueChanged.connect(lambda v, r=row: self._queue_gain_changed(r, v))
            remove_btn = self.queue_table.cellWidget(row, 7)
            if isinstance(remove_btn, QtWidgets.QPushButton):
                try:
                    remove_btn.clicked.disconnect()
                except TypeError:
                    pass
                remove_btn.clicked.connect(lambda checked, r=row: self._remove_queue_row(r))

    def _queue_gain_changed(self, row: int, value: int):
        if self._get_selected_row() == row and self.current_file:
            self.player.set_gain_db(value)

    def _get_selected_row(self) -> int:
        rows = self.queue_table.selectionModel().selectedRows()
        return rows[0].row() if rows else -1

    def _get_queue_item(self, row: int) -> tuple[Path, int] | None:
        if row < 0 or row >= self.queue_table.rowCount():
            return None
        item = self.queue_table.item(row, 1)
        if not item:
            return None
        path = Path(item.data(QtCore.Qt.ItemDataRole.UserRole))
        gain_spin = self.queue_table.cellWidget(row, 2)
        gain = gain_spin.value() if isinstance(gain_spin, QtWidgets.QSpinBox) else DEFAULT_GAIN_DB
        return (path, gain)

    def _get_row_isolate_checked(self, row: int) -> bool:
        if row < 0 or row >= self.queue_table.rowCount():
            return False
        w = self.queue_table.cellWidget(row, 3)
        return w.isChecked() if isinstance(w, QtWidgets.QCheckBox) else False

    def _get_row_transcribe_checked(self, row: int) -> bool:
        if row < 0 or row >= self.queue_table.rowCount():
            return False
        w = self.queue_table.cellWidget(row, 4)
        return w.isChecked() if isinstance(w, QtWidgets.QCheckBox) else False

    def _get_row_warmth_checked(self, row: int) -> bool:
        if row < 0 or row >= self.queue_table.rowCount():
            return False
        w = self.queue_table.cellWidget(row, 5)
        return w.isChecked() if isinstance(w, QtWidgets.QCheckBox) else False

    def _get_row_dpdf_checked(self, row: int) -> bool:
        if row < 0 or row >= self.queue_table.rowCount():
            return False
        w = self.queue_table.cellWidget(row, 6)
        return w.isChecked() if isinstance(w, QtWidgets.QCheckBox) else False

    def _pick_files_to_add(self):
        paths, _ = QtWidgets.QFileDialog.getOpenFileNames(
            self,
            i18n.tr("dialog.select_audio"),
            "",
            i18n.tr("dialog.audio_filter"),
        )
        if paths:
            self._add_files_to_queue([Path(p) for p in paths])

    def _on_queue_selection_changed(self):
        row = self._get_selected_row()
        if row >= 0:
            self._load_file_from_queue(row)

    def _load_file_from_queue(self, row: int):
        item = self._get_queue_item(row)
        if item:
            path, gain = item
            self._load_file(path, gain)

    def _load_file(self, path: Path, gain_db: int | None = None):
        self._cancel_preview_process(force=True)
        self.current_file = path
        self._original_media_path = path
        gain = gain_db if gain_db is not None else DEFAULT_GAIN_DB
        self.player.set_media(path)
        self.player.set_gain_db(gain)
        self.log_output.appendPlainText(i18n.tr("log.loaded", path=str(path)))
        self.player.preview_button.setEnabled(True)
        self.player.return_button.setEnabled(False)
        self._preview_path = None
        self._preview_return_position_ms = 0
        self._pending_restore_position = None
        self._pending_restore_resume = False
        self._update_transcribe_button()

    def _get_vibe_path(self) -> str:
        """Chemin vers l'exécutable Vibe/sona (personnalisé ou défaut dans le PATH)."""
        if self._vibe_path_cached is not None:
            return self._vibe_path_cached
        settings = QtCore.QSettings()
        raw = vibe.get_vibe_path_from_settings(settings, VIBE_CMD)
        if not raw or raw == VIBE_CMD:
            self._vibe_path_cached = VIBE_CMD
            return VIBE_CMD
        resolved = vibe.resolve_vibe_exe(settings, VIBE_CMD)
        if resolved != raw and "sona" in resolved.lower() and not self._sona_auto_detected_logged and self.log_output:
            self.log_output.appendPlainText("INFO: Utilisation de sona.exe au lieu de vibe.exe")
            self._sona_auto_detected_logged = True
        self._vibe_path_cached = resolved
        return resolved

    def _vibe_path_is_set(self) -> bool:
        """True si l'utilisateur a défini un chemin personnalisé (exécutable Vibe/sona)."""
        settings = QtCore.QSettings()
        raw = vibe.get_vibe_path_from_settings(settings, VIBE_CMD)
        return bool(raw and raw.strip() and raw != VIBE_CMD)

    def _get_vibe_model(self) -> str:
        return vibe.get_vibe_model(QtCore.QSettings())

    def _get_vibe_language(self) -> str:
        return vibe.get_vibe_language(QtCore.QSettings())

    def _get_vibe_format(self) -> str:
        return vibe.get_vibe_format(QtCore.QSettings())

    def _get_conversion_params(self) -> dict:
        """Charge les paramètres de conversion depuis QSettings."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        def v(key, default):
            val = settings.value(key, default)
            if val is None:
                return default
            return val
        try:
            afftdn_nf = int(v(SETTINGS_AFFTDN_NF, DEFAULT_AFFTDN_NF))
        except (TypeError, ValueError):
            afftdn_nf = DEFAULT_AFFTDN_NF
        try:
            comp_threshold = float(v(SETTINGS_COMP_THRESHOLD, DEFAULT_COMP_THRESHOLD_DB))
        except (TypeError, ValueError):
            comp_threshold = DEFAULT_COMP_THRESHOLD_DB
        try:
            comp_ratio = float(v(SETTINGS_COMP_RATIO, DEFAULT_COMP_RATIO))
        except (TypeError, ValueError):
            comp_ratio = DEFAULT_COMP_RATIO
        try:
            comp_attack = int(v(SETTINGS_COMP_ATTACK, DEFAULT_COMP_ATTACK_MS))
        except (TypeError, ValueError):
            comp_attack = DEFAULT_COMP_ATTACK_MS
        try:
            comp_release = int(v(SETTINGS_COMP_RELEASE, DEFAULT_COMP_RELEASE_MS))
        except (TypeError, ValueError):
            comp_release = DEFAULT_COMP_RELEASE_MS
        opus_bitrate = str(v(SETTINGS_OPUS_BITRATE, DEFAULT_OPUS_BITRATE) or DEFAULT_OPUS_BITRATE).strip()
        opus_preview = str(v(SETTINGS_OPUS_PREVIEW_BITRATE, DEFAULT_OPUS_PREVIEW_BITRATE) or DEFAULT_OPUS_PREVIEW_BITRATE).strip()
        settings.endGroup()
        return {
            "afftdn_nf": afftdn_nf,
            "comp_threshold_db": comp_threshold,
            "comp_ratio": comp_ratio,
            "comp_attack_ms": comp_attack,
            "comp_release_ms": comp_release,
            "opus_bitrate": opus_bitrate or DEFAULT_OPUS_BITRATE,
            "opus_preview_bitrate": opus_preview or DEFAULT_OPUS_PREVIEW_BITRATE,
        }

    def _uvr5_enabled(self) -> bool:
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        en = settings.value(SETTINGS_UVR5_ENABLED, False)
        settings.endGroup()
        return bool(en)

    def _list_vibe_models(self) -> list[str]:
        return vibe.list_vibe_models()

    def _run_vibe_check_once(self):
        """Lance le test Vibe une fois (timer), met en cache, met à jour le bouton et les cases."""
        self._vibe_check_scheduled = False
        try:
            self._vibe_available()
        except Exception:
            self._vibe_available_cached = False
        self._update_transcribe_button()
        self._update_transcribe_checkboxes()

    def _update_transcribe_checkboxes(self):
        """Active ou désactive les cases « Transcrire » selon la dispo Vibe."""
        vibe_ok = getattr(self, "_vibe_available_cached", None) is True
        uvr5_ok = getattr(self, "_uvr5_available_cached", None) is True
        for row in range(self.queue_table.rowCount()):
            # Checkbox "Isoler voix" (colonne 3)
            isolate_w = self.queue_table.cellWidget(row, 3)
            if isinstance(isolate_w, QtWidgets.QCheckBox):
                isolate_w.setEnabled(uvr5_ok)
                if not uvr5_ok:
                    isolate_w.setChecked(False)
            
            # Checkbox "Transcrire" (colonne 4)
            w = self.queue_table.cellWidget(row, 4)
            if isinstance(w, QtWidgets.QCheckBox):
                w.setEnabled(vibe_ok)
                if not vibe_ok:
                    w.setChecked(False)
            # Checkbox "Chaleur" (colonne 5)
            warmth_ok = warmth.is_pedalboard_available()
            warmth_w = self.queue_table.cellWidget(row, 5)
            if isinstance(warmth_w, QtWidgets.QCheckBox):
                warmth_w.setEnabled(warmth_ok)
                if not warmth_ok:
                    warmth_w.setChecked(False)
            # Checkbox "Nettoyer DPDF" (colonne 6)
            dpdf_ok = dpdf.is_dpdf_available()
            dpdf_w = self.queue_table.cellWidget(row, 6)
            if isinstance(dpdf_w, QtWidgets.QCheckBox):
                dpdf_w.setEnabled(dpdf_ok)
                if not dpdf_ok:
                    dpdf_w.setChecked(False)

    def _get_dpdf_settings(self) -> tuple[str, float | None]:
        """Retourne (model, attn_limit_db ou None si désactivé)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        model = str(settings.value(SETTINGS_DPDF_MODEL, DEFAULT_DPDF_MODEL) or DEFAULT_DPDF_MODEL).strip()
        try:
            attn = float(settings.value(SETTINGS_DPDF_ATTN_LIMIT_DB, DEFAULT_DPDF_ATTN_LIMIT_DB))
        except (TypeError, ValueError):
            attn = DEFAULT_DPDF_ATTN_LIMIT_DB
        settings.endGroup()
        if model not in dpdf.AVAILABLE_MODELS:
            model = DEFAULT_DPDF_MODEL
        return model, (attn if attn > 0 else None)

    def _apply_dpdf_step(self, input_path: Path, output_dir: Path | None = None) -> Path:
        """
        Applique DPDF si demandé pour la conversion en cours.
        Retourne le chemin à utiliser ensuite (nettoyé ou d'origine).
        Met à jour self._conversion_dpdf_applied en cas de succès.
        """
        if not self._conversion_apply_dpdf:
            return input_path
        if not dpdf.is_dpdf_available():
            self.log_output.appendPlainText(i18n.tr("log.dpdf_skipped"))
            return input_path
        out_dir = output_dir or input_path.parent
        out_dir.mkdir(parents=True, exist_ok=True)
        dpdf_out = out_dir / f"{input_path.stem}_dpdf.wav"
        model, attn = self._get_dpdf_settings()
        self.log_output.appendPlainText(i18n.tr("log.dpdf_start", name=input_path.name))
        if dpdf.apply_dpdf(input_path, dpdf_out, model=model, attn_limit_db=attn):
            self.log_output.appendPlainText(i18n.tr("log.dpdf_done", path=str(dpdf_out)))
            self._conversion_dpdf_applied = True
            return dpdf_out
        self.log_output.appendPlainText(i18n.tr("log.dpdf_failed"))
        return input_path

    def _update_transcribe_button(self):
        if getattr(self, "_vibe_available_cached", None) is None and not getattr(
            self, "_vibe_check_scheduled", False
        ):
            self._vibe_check_scheduled = True
            QtCore.QTimer.singleShot(600, self._run_vibe_check_once)
            vibe_ok = False
        else:
            try:
                vibe_ok = self._vibe_available()
            except Exception:
                vibe_ok = False
        has_any = self.queue_table.rowCount() > 0
        proc = getattr(self, "_vibe_process", None)
        busy = proc is not None and proc.state() != QtCore.QProcess.ProcessState.NotRunning
        ffmpeg_proc = getattr(self, "_ffmpeg_process", None)
        ffmpeg_busy = ffmpeg_proc is not None and ffmpeg_proc.state() != QtCore.QProcess.ProcessState.NotRunning
        # Bouton actif si test OK *ou* si un chemin personnalisé est défini (évite bouton grisé)
        can_transcribe = vibe_ok or self._vibe_path_is_set()
        self.transcribe_button.setEnabled(can_transcribe and has_any and not busy and not ffmpeg_busy)
        self.transcribe_button.setToolTip(
            i18n.tr("transcribe.tooltip") if can_transcribe else i18n.tr("transcribe.vibe_not_installed")
        )
        # Bouton "Tout traiter" actif si on a des fichiers et que Vibe est dispo
        if hasattr(self, 'process_all_button'):
            self.process_all_button.setEnabled(can_transcribe and has_any and not busy and not ffmpeg_busy)
        self._update_transcribe_checkboxes()

    def _update_duration(self, duration_ms: int):
        self._current_duration_sec = self.player.duration_seconds()

    def _get_current_gain(self) -> int:
        row = self._get_selected_row()
        if row >= 0:
            item = self._get_queue_item(row)
            if item:
                return item[1]
        return DEFAULT_GAIN_DB

    def _preview_segment(self):
        if not self.current_file:
            QtWidgets.QMessageBox.warning(self, i18n.tr("msg.no_file_title"), i18n.tr("msg.preview_no_file"))
            return

        if not self._ffmpeg_available():
            QtWidgets.QMessageBox.critical(self, i18n.tr("msg.ffmpeg_missing_title"), i18n.tr("msg.ffmpeg_preview"))
            return

        if self._preview_process:
            QtWidgets.QMessageBox.information(
                self,
                i18n.tr("msg.preview_busy_title"),
                i18n.tr("msg.preview_busy_body"),
            )
            return

        duration_sec = self.player.duration_seconds()
        position_ms = self.player.media_player.position()
        start_sec = position_ms / 1000 if position_ms > 0 else 0.0

        if duration_sec and duration_sec > 0:
            remaining = max(0.0, duration_sec - start_sec)
            if remaining <= 0:
                start_sec = max(0.0, duration_sec - 5.0)
                remaining = max(0.0, duration_sec - start_sec)
            snippet_duration = min(5.0, remaining) if remaining > 0 else 5.0
        else:
            snippet_duration = 5.0

        if snippet_duration <= 0:
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("msg.segment_unavailable_title"),
                i18n.tr("msg.segment_unavailable_body"),
            )
            return

        self._preview_return_position_ms = position_ms
        self._preview_snippet_duration_ms = int(snippet_duration * 1000)
        self._preview_was_playing = (
            self.player.media_player.playbackState()
            == QtMultimedia.QMediaPlayer.PlaybackState.PlayingState
        )
        self.player.media_player.pause()
        self.player.preview_button.setEnabled(False)
        self.player.return_button.setEnabled(False)
        self.player.info_label.setText(i18n.tr("player.preparing"))
        self._using_preview = False

        self._render_preview(start_sec, snippet_duration)

    def _render_preview(self, start_sec: float, duration_sec: float):
        if not self.current_file:
            return

        if not self._ffmpeg_available():
            self.log_output.appendPlainText(i18n.tr("log.ffmpeg_required_preview"))
            self.player.preview_button.setEnabled(True)
            return

        self._cancel_preview_process(force=True)

        input_path = self._original_media_path or self.current_file
        gain = self._get_current_gain()
        preview_name = f"{self.current_file.stem}_preview_{int(start_sec * 1000)}.opus"
        self._preview_path = self._preview_dir / preview_name

        params = self._get_conversion_params()
        row = self._get_selected_row()
        if row >= 0 and self._get_row_dpdf_checked(row) and dpdf.is_dpdf_available():
            params = {**params, "skip_afftdn": True}

        command = ffmpeg.build_preview_command(
            input_path, self._preview_path, start_sec, duration_sec, gain,
            params,
        )

        self.log_output.appendPlainText(
            i18n.tr("log.preview_encoding", duration=duration_sec, start=start_sec)
        )

        process = QtCore.QProcess(self)
        process.setProgram(command[0])
        process.setArguments(command[1:])
        process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
        process.finished.connect(self._preview_finished)
        process.errorOccurred.connect(
            lambda _error: self.log_output.appendPlainText(i18n.tr("log.preview_error"))
        )
        self._preview_process = process
        process.start()

        if not process.waitForStarted(3000):
            self.log_output.appendPlainText(i18n.tr("log.preview_start_failed"))
            process.deleteLater()
            self._preview_process = None
            self.player.preview_button.setEnabled(True)

    def _preview_finished(self, exit_code: int, exit_status: QtCore.QProcess.ExitStatus):
        process = self._preview_process
        self._preview_process = None
        if process:
            process.deleteLater()

        success = (
            exit_status == QtCore.QProcess.ExitStatus.NormalExit
            and exit_code == 0
            and self._preview_path
            and self._preview_path.exists()
        )

        self.player.preview_button.setEnabled(True)

        if success:
            play_path = self._preview_path
            row = self._get_selected_row()
            apply_dpdf_preview = row >= 0 and self._get_row_dpdf_checked(row) and dpdf.is_dpdf_available()
            apply_warmth_preview = (
                row >= 0
                and self._get_row_warmth_checked(row)
                and warmth.is_pedalboard_available()
            )
            # Appliquer DPDF puis chaleur à la prévisualisation si les options sont activées
            if apply_dpdf_preview or apply_warmth_preview:
                wav_path = self._preview_dir / f"{self._preview_path.stem}_dec.wav"
                try:
                    subprocess.run(
                        [
                            FFMPEG_CMD, "-y", "-i", str(self._preview_path),
                            "-vn", "-acodec", "pcm_s16le", str(wav_path),
                        ],
                        check=True,
                        capture_output=True,
                        timeout=30,
                    )
                    work_path = wav_path
                    extras: list[str] = []
                    if apply_dpdf_preview:
                        dpdf_path = self._preview_dir / f"{self._preview_path.stem}_dpdf.wav"
                        model, attn = self._get_dpdf_settings()
                        if dpdf.apply_dpdf(work_path, dpdf_path, model=model, attn_limit_db=attn):
                            work_path = dpdf_path
                            extras.append("DPDF")
                    if apply_warmth_preview:
                        warm_path = self._preview_dir / f"{self._preview_path.stem}_warm.wav"
                        settings = QtCore.QSettings()
                        settings.beginGroup(i18n.SETTINGS_GROUP)
                        high_shelf_gain = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_GAIN, DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB))
                        high_shelf_cutoff = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_CUTOFF, DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ))
                        distortion_drive = float(settings.value(SETTINGS_WARMTH_DISTORTION_DRIVE, DEFAULT_WARMTH_DISTORTION_DRIVE_DB))
                        compressor_threshold = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_THRESHOLD, DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB))
                        compressor_ratio = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_RATIO, DEFAULT_WARMTH_COMPRESSOR_RATIO))
                        final_gain = float(settings.value(SETTINGS_WARMTH_FINAL_GAIN, DEFAULT_WARMTH_FINAL_GAIN_DB))
                        settings.endGroup()
                        if warmth.apply_warmth(
                            work_path, warm_path,
                            high_shelf_gain_db=high_shelf_gain,
                            high_shelf_cutoff_hz=high_shelf_cutoff,
                            distortion_drive_db=distortion_drive,
                            compressor_threshold_db=compressor_threshold,
                            compressor_ratio=compressor_ratio,
                            final_gain_db=final_gain,
                        ):
                            work_path = warm_path
                            extras.append("chaleur")
                    if work_path != wav_path or extras:
                        play_path = work_path
                        suffix = " + ".join(extras) if extras else ""
                        self.log_output.appendPlainText(
                            i18n.tr("log.preview_ready") + (f" (avec {suffix})" if suffix else "")
                        )
                except (subprocess.CalledProcessError, FileNotFoundError, OSError) as e:
                    self.log_output.appendPlainText(f"Prévisualisation post-traitement ignorée: {e}")
                finally:
                    if wav_path.exists() and play_path != wav_path:
                        wav_path.unlink(missing_ok=True)
            if play_path == self._preview_path:
                self.log_output.appendPlainText(i18n.tr("log.preview_ready"))
            self._using_preview = True
            self.player.set_media(play_path)
            self.player.set_gain_db(self._get_current_gain())
            self.player.media_player.setPosition(0)
            self.player.media_player.play()
            self.player.play_button.setText(self.player.get_pause_text())
            self.player.return_button.setEnabled(True)
            self.player.info_label.setText(i18n.tr("player.preview_label", name=play_path.name))
        else:
            if exit_status != QtCore.QProcess.ExitStatus.NormalExit:
                self.log_output.appendPlainText(i18n.tr("log.preview_interrupted"))
            self.player.return_button.setEnabled(self._using_preview)
            if not self._using_preview and self._original_media_path:
                self.player.info_label.setText(self._original_media_path.name)

    def _restore_original_file(self, auto: bool = False):
        if not self._original_media_path or not self._using_preview:
            return

        resume = self._preview_was_playing
        self._using_preview = False
        self.player.set_media(self._original_media_path)
        self.player.set_gain_db(self._get_current_gain())

        duration = self.player.media_player.duration()
        target_pos = self._preview_return_position_ms
        if duration > 0:
            target_pos = min(target_pos, duration)

        self._pending_restore_position = target_pos
        self._pending_restore_resume = resume
        if not resume:
            self.player.play_button.setText(self.player.get_play_text())

        self.player.return_button.setEnabled(False)
        self.player.preview_button.setEnabled(True)
        self.player.info_label.setText(self._original_media_path.name)

        self._preview_was_playing = False

    def _handle_media_status(self, status: QtMultimedia.QMediaPlayer.MediaStatus):
        if (
            self._using_preview
            and status == QtMultimedia.QMediaPlayer.MediaStatus.EndOfMedia
        ):
            self._restore_original_file(auto=True)
            return

        if status in (
            QtMultimedia.QMediaPlayer.MediaStatus.LoadedMedia,
            QtMultimedia.QMediaPlayer.MediaStatus.BufferedMedia,
        ):
            if self._pending_restore_position is not None:
                self.player.media_player.setPosition(self._pending_restore_position)
                if self._pending_restore_resume:
                    self.player.media_player.play()
                    self.player.play_button.setText(self.player.get_pause_text())
                self._pending_restore_position = None
                self._pending_restore_resume = False

    def _cancel_preview_process(self, force: bool = False):
        if not self._preview_process:
            return

        if force:
            self._preview_process.kill()
        else:
            self._preview_process.terminate()
            if not self._preview_process.waitForFinished(2000):
                self._preview_process.kill()
        self._preview_process.waitForFinished(2000)
        self._preview_process.deleteLater()
        self._preview_process = None
        self.player.preview_button.setEnabled(True)
    def _remember_output_dir(self, text: str):
        self._output_dir = Path(text).expanduser() if text else None

    def _pick_output_dir(self):
        directory = QtWidgets.QFileDialog.getExistingDirectory(self, i18n.tr("dialog.select_output_dir"))
        if directory:
            self.output_dir_edit.setText(directory)

    def _process_all(self):
        """Convertit tous les fichiers puis transcrit ceux cochés."""
        if not self._ffmpeg_available():
            QtWidgets.QMessageBox.critical(
                self,
                i18n.tr("msg.ffmpeg_missing_title"),
                i18n.tr("msg.ffmpeg_missing_body"),
            )
            return
        
        if not self._vibe_path_is_set() and not self._vibe_available():
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("transcribe.vibe_missing_title"),
                i18n.tr("transcribe.vibe_missing_body"),
            )
            return
        
        # Vérifier s'il y a des fichiers cochés pour transcription
        has_checked = False
        for row in range(self.queue_table.rowCount()):
            if self._get_row_transcribe_checked(row):
                has_checked = True
                break
        
        if not has_checked:
            # Demander confirmation si aucun fichier n'est coché
            reply = QtWidgets.QMessageBox.question(
                self,
                "Aucun fichier coché",
                "Aucun fichier n'est coché pour la transcription.\n\nVoulez-vous seulement convertir en Opus ?",
                QtWidgets.QMessageBox.StandardButton.Yes | QtWidgets.QMessageBox.StandardButton.No,
                QtWidgets.QMessageBox.StandardButton.No
            )
            if reply == QtWidgets.QMessageBox.StandardButton.No:
                return
            self._process_all_transcribe_after = False
        else:
            self._process_all_transcribe_after = True
        
        self._process_all_mode = True
        self.log_output.appendPlainText("=== Traitement complet ===")
        self._convert_audio()

    def _transcribe_current(self):
        if not self._vibe_path_is_set() and not self._vibe_available():
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("transcribe.vibe_missing_title"),
                i18n.tr("transcribe.vibe_missing_body"),
            )
            return
        if self._vibe_process and self._vibe_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            QtWidgets.QMessageBox.information(
                self,
                i18n.tr("transcribe.busy_title"),
                i18n.tr("transcribe.busy_body"),
            )
            return
        fmt = self._get_vibe_format()
        ext = f".{fmt}"
        self._transcribe_queue = []
        for row in range(self.queue_table.rowCount()):
            if not self._get_row_transcribe_checked(row):
                continue
            item = self._get_queue_item(row)
            if item and item[0].exists():
                inp = item[0]
                out_path = inp.parent / f"{inp.stem}{ext}"
                self._transcribe_queue.append((inp, out_path))
        if not self._transcribe_queue and self.current_file and self.current_file.exists():
            ext = f".{self._get_vibe_format()}"
            self._transcribe_queue = [
                (self.current_file, self.current_file.parent / f"{self.current_file.stem}{ext}")
            ]
        if not self._transcribe_queue:
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("msg.no_file_title"),
                i18n.tr("transcribe.no_file_or_checked"),
            )
            return
        self._current_transcribe_index = 0
        self.transcribe_button.setEnabled(False)
        self._start_next_transcribe()

    def _start_next_transcribe(self):
        if self._current_transcribe_index < 0 or self._current_transcribe_index >= len(self._transcribe_queue):
            self._on_transcribe_queue_done()
            return
        input_path, output_path = self._transcribe_queue[self._current_transcribe_index]
        self._transcribe_output_path = output_path
        vibe_exe = self._get_vibe_path()
        model = self._get_vibe_model()
        language = self._get_vibe_language()
        fmt = self._get_vibe_format()
        sona_style = self._vibe_uses_sona_syntax()
        self._vibe_stdout_file = None
        if sona_style:
            if not model:
                self.log_output.appendPlainText(i18n.tr("transcribe.model_required_sona"))
                QtWidgets.QMessageBox.warning(
                    self,
                    i18n.tr("transcribe.vibe_missing_title"),
                    i18n.tr("transcribe.model_required_sona"),
                )
                self._current_transcribe_index += 1
                QtCore.QTimer.singleShot(0, self._start_next_transcribe)
                return
            command = [vibe_exe, "transcribe", model, str(input_path)]
            if language:
                command.extend(["-l", language])
            try:
                self._vibe_stdout_file = open(output_path, "w", encoding="utf-8")
            except OSError as e:
                self.log_output.appendPlainText(str(e))
                self._current_transcribe_index += 1
                QtCore.QTimer.singleShot(0, self._start_next_transcribe)
                return
        else:
            command = [vibe_exe, "transcribe"]
            if model:
                command.extend(["--model", model])
            command.extend(["--file", str(input_path), "--write", str(output_path)])
            if fmt:
                command.extend(["--format", fmt])
            if language:
                command.extend(["-l", language])
        total = len(self._transcribe_queue)
        idx = self._current_transcribe_index + 1
        self.log_output.appendPlainText(
            i18n.tr("transcribe.started_n", name=input_path.name, index=idx, total=total)
        )
        self._vibe_process = QtCore.QProcess(self)
        self._vibe_process.setProgram(command[0])
        self._vibe_process.setArguments(command[1:])
        self._vibe_process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
        self._vibe_process.readyReadStandardError.connect(self._on_vibe_output)
        self._vibe_process.readyReadStandardOutput.connect(self._on_vibe_output)
        self._vibe_process.finished.connect(self._vibe_finished)
        self._vibe_process.start()
        if not self._vibe_process.waitForStarted(5000):
            self.log_output.appendPlainText(i18n.tr("transcribe.start_failed"))
            self._vibe_process = None
            self._current_transcribe_index += 1
            QtCore.QTimer.singleShot(0, self._start_next_transcribe)
            return

    def _on_vibe_output(self):
        if not self._vibe_process:
            return
        stdout_data = self._vibe_process.readAllStandardOutput()
        stderr_data = self._vibe_process.readAllStandardError()
        out_file = getattr(self, "_vibe_stdout_file", None)
        if stdout_data:
            if out_file:
                try:
                    out_file.write(stdout_data.data().decode(errors="replace"))
                    out_file.flush()
                except OSError:
                    pass
            else:
                text = bytes(stdout_data).decode(errors="ignore").strip()
                if text:
                    self.log_output.appendPlainText(text)
        if stderr_data:
            text = bytes(stderr_data).decode(errors="ignore").strip()
            if text:
                self.log_output.appendPlainText(text)

    def _vibe_finished(self, exit_code: int, exit_status: QtCore.QProcess.ExitStatus):
        process = self._vibe_process
        self._vibe_process = None
        out_path = self._transcribe_output_path
        self._transcribe_output_path = None
        out_file = getattr(self, "_vibe_stdout_file", None)
        self._vibe_stdout_file = None
        if out_file:
            try:
                out_file.close()
            except OSError:
                pass
        if process:
            process.deleteLater()
        success = exit_status == QtCore.QProcess.ExitStatus.NormalExit and exit_code == 0
        if success and out_path and out_path.exists():
            self.log_output.appendPlainText(i18n.tr("transcribe.done", path=str(out_path)))
        elif not success:
            self.log_output.appendPlainText(i18n.tr("transcribe.failed"))
        self._current_transcribe_index += 1
        if self._current_transcribe_index < len(self._transcribe_queue):
            QtCore.QTimer.singleShot(0, self._start_next_transcribe)
        else:
            self._on_transcribe_queue_done()

    def _on_transcribe_queue_done(self):
        self._transcribe_queue = []
        self._current_transcribe_index = -1
        self._update_transcribe_button()
        QtWidgets.QMessageBox.information(
            self,
            i18n.tr("transcribe.done_title"),
            i18n.tr("transcribe.queue_done"),
        )

    def _show_vibe_path_dialog(self):
        """Ouvre la boîte de dialogue pour définir le chemin vers Vibe."""
        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("vibe.path_dialog_title"))
        layout = QtWidgets.QVBoxLayout(dialog)
        form = QtWidgets.QFormLayout()
        path_edit = QtWidgets.QLineEdit()
        settings = QtCore.QSettings()
        raw = vibe.get_vibe_path_from_settings(settings, VIBE_CMD)
        path_edit.setText(raw if raw != VIBE_CMD else "")
        path_edit.setPlaceholderText(VIBE_CMD + " (" + i18n.tr("vibe.use_path") + ")")
        path_edit.setToolTip(i18n.tr("vibe.tooltip_path"))
        form.addRow(i18n.tr("vibe.path_label"), path_edit)
        layout.addLayout(form)
        btn_layout = QtWidgets.QHBoxLayout()
        browse_btn = QtWidgets.QPushButton(i18n.tr("vibe.browse"))
        use_path_btn = QtWidgets.QPushButton(i18n.tr("vibe.use_path"))
        btn_layout.addWidget(browse_btn)
        btn_layout.addWidget(use_path_btn)
        btn_layout.addStretch()
        layout.addLayout(btn_layout)
        use_path_btn.clicked.connect(lambda: path_edit.clear())
        def do_browse():
            path, _ = QtWidgets.QFileDialog.getOpenFileName(
                dialog,
                i18n.tr("vibe.browse_title"),
                path_edit.text() or "",
                "Executables (*.exe);;All (*.*)" if sys.platform == "win32" else "All (*.*)",
            )
            if path:
                path_edit.setText(path)
        browse_btn.clicked.connect(do_browse)
        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            path = path_edit.text().strip()
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_VIBE_PATH, path if path else "")
            settings.endGroup()
            settings.sync()
            self._vibe_available_cached = None
            self._vibe_sona_syntax_cached = None
            self._vibe_path_cached = None
            self._sona_auto_detected_logged = False
            self._vibe_check_scheduled = False
            self._update_transcribe_button()

    def _show_vibe_options_dialog(self):
        """Options de transcription : modèle Whisper, langue, format de sortie."""
        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("vibe.options_title"))
        layout = QtWidgets.QVBoxLayout(dialog)
        form = QtWidgets.QFormLayout()
        model_combo = QtWidgets.QComboBox()
        model_combo.setEditable(True)
        model_combo.setMinimumWidth(300)
        models_found = vibe.list_vibe_models()
        for m in models_found:
            model_combo.addItem(m, m)
        current_model = self._get_vibe_model()
        if current_model:
            idx = model_combo.findData(current_model)
            if idx >= 0:
                model_combo.setCurrentIndex(idx)
            else:
                model_combo.setEditText(current_model)
        model_combo.setPlaceholderText(i18n.tr("vibe.model_placeholder"))
        model_combo.setToolTip(i18n.tr("vibe.tooltip_model"))
        browse_model_btn = QtWidgets.QPushButton(i18n.tr("vibe.browse"))
        def browse_model():
            path, _ = QtWidgets.QFileDialog.getOpenFileName(
                dialog,
                i18n.tr("vibe.model_browse_title"),
                model_combo.currentText() or "",
                "Whisper model (*.bin);;All (*.*)",
            )
            if path:
                model_combo.setEditText(path)
        browse_model_btn.clicked.connect(browse_model)
        model_row = QtWidgets.QHBoxLayout()
        model_row.addWidget(model_combo)
        model_row.addWidget(browse_model_btn)
        form.addRow(i18n.tr("vibe.model_label"), model_row)
        lang_combo = QtWidgets.QComboBox()
        lang_combo.setToolTip(i18n.tr("vibe.tooltip_language"))
        for cli_val, label_key in VIBE_LANGUAGES:
            if label_key == "auto":
                lang_combo.addItem(i18n.tr("vibe.lang_auto"), cli_val)
            else:
                lang_combo.addItem(i18n.tr(f"vibe.lang_{label_key}"), cli_val)
        current_lang = self._get_vibe_language()
        idx = lang_combo.findData(current_lang)
        if idx >= 0:
            lang_combo.setCurrentIndex(idx)
        form.addRow(i18n.tr("vibe.language_label"), lang_combo)
        format_combo = QtWidgets.QComboBox()
        format_combo.setToolTip(i18n.tr("vibe.tooltip_format"))
        for f in VIBE_FORMATS:
            format_combo.addItem(f.upper(), f)
        fmt_current = self._get_vibe_format()
        fmt_idx = format_combo.findData(fmt_current)
        if fmt_idx >= 0:
            format_combo.setCurrentIndex(fmt_idx)
        form.addRow(i18n.tr("vibe.format_label"), format_combo)
        layout.addLayout(form)
        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_VIBE_MODEL, model_combo.currentText().strip())
            settings.setValue(SETTINGS_VIBE_LANGUAGE, lang_combo.currentData() or "")
            settings.setValue(SETTINGS_VIBE_FORMAT, format_combo.currentData() or "txt")
            settings.endGroup()
            settings.sync()

    def _show_conversion_settings_dialog(self):
        """Fenêtre de paramétrage compression et effets (Opus)."""
        params = self._get_conversion_params()
        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("conv.dialog_title"))
        layout = QtWidgets.QVBoxLayout(dialog)
        
        # Presets
        preset_layout = QtWidgets.QHBoxLayout()
        preset_label = QtWidgets.QLabel(i18n.tr("conv.preset"))
        preset_combo = QtWidgets.QComboBox()
        preset_combo.addItem(i18n.tr("conv.preset_custom"), "custom")
        preset_combo.addItem(i18n.tr("conv.preset_audiobook"), "audiobook")
        preset_combo.addItem(i18n.tr("conv.preset_podcast"), "podcast")
        preset_combo.addItem(i18n.tr("conv.preset_voicechat"), "voicechat")
        preset_combo.addItem(i18n.tr("conv.preset_voicenote"), "voicenote")
        preset_layout.addWidget(preset_label)
        preset_layout.addWidget(preset_combo)
        layout.addLayout(preset_layout)
        
        form = QtWidgets.QFormLayout()
        afftdn_spin = QtWidgets.QSpinBox()
        afftdn_spin.setRange(-60, 0)
        afftdn_spin.setValue(params["afftdn_nf"])
        afftdn_spin.setSuffix(" dB")
        afftdn_spin.setToolTip(i18n.tr("conv.tooltip_afftdn_nf"))
        form.addRow(i18n.tr("conv.afftdn_nf"), afftdn_spin)
        comp_thresh_spin = QtWidgets.QDoubleSpinBox()
        comp_thresh_spin.setRange(-60, 0)
        comp_thresh_spin.setValue(params["comp_threshold_db"])
        comp_thresh_spin.setSuffix(" dB")
        comp_thresh_spin.setToolTip(i18n.tr("conv.tooltip_comp_threshold"))
        form.addRow(i18n.tr("conv.comp_threshold"), comp_thresh_spin)
        comp_ratio_spin = QtWidgets.QDoubleSpinBox()
        comp_ratio_spin.setRange(1.0, 20.0)
        comp_ratio_spin.setDecimals(1)
        comp_ratio_spin.setValue(params["comp_ratio"])
        comp_ratio_spin.setToolTip(i18n.tr("conv.tooltip_comp_ratio"))
        form.addRow(i18n.tr("conv.comp_ratio"), comp_ratio_spin)
        comp_attack_spin = QtWidgets.QSpinBox()
        comp_attack_spin.setRange(1, 2000)
        comp_attack_spin.setValue(params["comp_attack_ms"])
        comp_attack_spin.setSuffix(" ms")
        comp_attack_spin.setToolTip(i18n.tr("conv.tooltip_comp_attack"))
        form.addRow(i18n.tr("conv.comp_attack"), comp_attack_spin)
        comp_release_spin = QtWidgets.QSpinBox()
        comp_release_spin.setRange(10, 5000)
        comp_release_spin.setValue(params["comp_release_ms"])
        comp_release_spin.setSuffix(" ms")
        comp_release_spin.setToolTip(i18n.tr("conv.tooltip_comp_release"))
        form.addRow(i18n.tr("conv.comp_release"), comp_release_spin)
        opus_edit = QtWidgets.QLineEdit()
        opus_edit.setText(params["opus_bitrate"])
        opus_edit.setPlaceholderText("20k")
        opus_edit.setToolTip(i18n.tr("conv.tooltip_opus_bitrate"))
        form.addRow(i18n.tr("conv.opus_bitrate"), opus_edit)
        opus_preview_edit = QtWidgets.QLineEdit()
        opus_preview_edit.setText(params["opus_preview_bitrate"])
        opus_preview_edit.setPlaceholderText("64k")
        opus_preview_edit.setToolTip(i18n.tr("conv.tooltip_opus_preview"))
        form.addRow(i18n.tr("conv.opus_preview_bitrate"), opus_preview_edit)
        layout.addLayout(form)
        
        # Fonction pour appliquer un preset
        def apply_preset(index):
            preset = preset_combo.itemData(index)
            if preset == "audiobook":
                # Audiobook : Qualité maximale
                afftdn_spin.setValue(-25)
                comp_thresh_spin.setValue(-18)
                comp_ratio_spin.setValue(2.0)
                comp_attack_spin.setValue(200)
                comp_release_spin.setValue(1000)
                opus_edit.setText("32k")
                opus_preview_edit.setText("64k")
            elif preset == "podcast":
                # Podcast : Standard (défaut)
                afftdn_spin.setValue(-25)
                comp_thresh_spin.setValue(-21)
                comp_ratio_spin.setValue(2.5)
                comp_attack_spin.setValue(200)
                comp_release_spin.setValue(1000)
                opus_edit.setText("20k")
                opus_preview_edit.setText("64k")
            elif preset == "voicechat":
                # Voice Chat : Léger
                afftdn_spin.setValue(-20)
                comp_thresh_spin.setValue(-21)
                comp_ratio_spin.setValue(3.0)
                comp_attack_spin.setValue(150)
                comp_release_spin.setValue(800)
                opus_edit.setText("16k")
                opus_preview_edit.setText("48k")
            elif preset == "voicenote":
                # Note vocale : Ultra-léger
                afftdn_spin.setValue(-18)
                comp_thresh_spin.setValue(-24)
                comp_ratio_spin.setValue(3.5)
                comp_attack_spin.setValue(100)
                comp_release_spin.setValue(500)
                opus_edit.setText("12k")
                opus_preview_edit.setText("32k")
        
        # Fonction pour passer en mode Custom quand on modifie manuellement
        def set_custom():
            preset_combo.setCurrentIndex(0)  # Custom
        
        preset_combo.currentIndexChanged.connect(apply_preset)
        afftdn_spin.valueChanged.connect(set_custom)
        comp_thresh_spin.valueChanged.connect(set_custom)
        comp_ratio_spin.valueChanged.connect(set_custom)
        comp_attack_spin.valueChanged.connect(set_custom)
        comp_release_spin.valueChanged.connect(set_custom)
        opus_edit.textChanged.connect(set_custom)
        opus_preview_edit.textChanged.connect(set_custom)
        
        def restore_defaults():
            afftdn_spin.setValue(DEFAULT_AFFTDN_NF)
            comp_thresh_spin.setValue(DEFAULT_COMP_THRESHOLD_DB)
            comp_ratio_spin.setValue(DEFAULT_COMP_RATIO)
            comp_attack_spin.setValue(DEFAULT_COMP_ATTACK_MS)
            comp_release_spin.setValue(DEFAULT_COMP_RELEASE_MS)
            opus_edit.setText(DEFAULT_OPUS_BITRATE)
            opus_preview_edit.setText(DEFAULT_OPUS_PREVIEW_BITRATE)
        btn_defaults = QtWidgets.QPushButton(i18n.tr("conv.restore_defaults"))
        btn_defaults.clicked.connect(restore_defaults)
        layout.addWidget(btn_defaults)
        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_AFFTDN_NF, afftdn_spin.value())
            settings.setValue(SETTINGS_COMP_THRESHOLD, comp_thresh_spin.value())
            settings.setValue(SETTINGS_COMP_RATIO, comp_ratio_spin.value())
            settings.setValue(SETTINGS_COMP_ATTACK, comp_attack_spin.value())
            settings.setValue(SETTINGS_COMP_RELEASE, comp_release_spin.value())
            settings.setValue(SETTINGS_OPUS_BITRATE, opus_edit.text().strip() or DEFAULT_OPUS_BITRATE)
            settings.setValue(SETTINGS_OPUS_PREVIEW_BITRATE, opus_preview_edit.text().strip() or DEFAULT_OPUS_PREVIEW_BITRATE)
            settings.endGroup()
            settings.sync()

    def _show_uvr5_dialog(self):
        """Fenêtre de paramétrage UVR5 / audio-separator (nettoyage voix)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        enabled = bool(settings.value(SETTINGS_UVR5_ENABLED, False))
        path = str(settings.value(SETTINGS_UVR5_PATH, "") or "").strip()
        chunk = int(settings.value(SETTINGS_UVR5_CHUNK_SEC, 1200) or 1200)
        # Migration automatique : si chunk=900, passer à 1200 pour éviter chunking sur fichiers de 15 min
        if chunk == 900:
            chunk = 1200
            settings.setValue(SETTINGS_UVR5_CHUNK_SEC, 1200)
        model = str(settings.value(SETTINGS_UVR5_MODEL, "") or "").strip()
        mdx_seg = int(settings.value(SETTINGS_UVR5_MDX_SEGMENT_SIZE, 2048) or 2048)
        mdxc_ov = int(settings.value(SETTINGS_UVR5_MDXC_OVERLAP, 8) or 8)
        mdx_bs = int(settings.value(SETTINGS_UVR5_MDX_BATCH_SIZE, 1) or 1)
        dml_dev = int(settings.value(SETTINGS_UVR5_DIRECTML_DEVICE, 1) or 1)  # Par défaut adapteur 1 (carte dédiée)
        stem = str(settings.value(SETTINGS_UVR5_STEM, "Vocals") or "Vocals").strip()
        settings.endGroup()
        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("uvr5.dialog_title"))
        layout = QtWidgets.QVBoxLayout(dialog)
        enabled_cb = QtWidgets.QCheckBox(i18n.tr("uvr5.enabled"))
        enabled_cb.setChecked(enabled)
        layout.addWidget(enabled_cb)
        form = QtWidgets.QFormLayout()
        path_edit = QtWidgets.QLineEdit()
        path_edit.setText(path)
        path_edit.setPlaceholderText("vide = PATH ; ou script .py / exe ; ou dossier venv")
        path_edit.setToolTip(i18n.tr("uvr5.tooltip_path"))
        form.addRow(i18n.tr("uvr5.path_label"), path_edit)
        chunk_spin = QtWidgets.QSpinBox()
        chunk_spin.setRange(60, 3600)
        chunk_spin.setValue(min(3600, max(60, chunk)))
        chunk_spin.setSuffix(" s")
        chunk_spin.setToolTip(i18n.tr("uvr5.tooltip_chunk"))
        form.addRow(i18n.tr("uvr5.chunk_label"), chunk_spin)
        
        # Sélection du stem à extraire
        stem_combo = QtWidgets.QComboBox()
        stems = ["Vocals", "Instrumental", "Drums", "Bass", "Guitar", "Piano", "Other"]
        for s in stems:
            stem_combo.addItem(s)
        stem_combo.setCurrentText(stem if stem in stems else "Vocals")
        stem_combo.setToolTip(i18n.tr("uvr5.tooltip_stem"))
        form.addRow("Stem à extraire:", stem_combo)
        
        # Sélection de l'adapteur DirectML (GPU)
        dml_combo = QtWidgets.QComboBox()
        dml_combo.addItem("Auto (défaut système)", -1)
        dml_combo.addItem("Adapteur 0 (généralement GPU intégré)", 0)
        dml_combo.addItem("Adapteur 1 (généralement carte dédiée)", 1)
        for i in range(2, 8):
            dml_combo.addItem(f"Adapteur {i}", i)
        # Sélectionner l'index actuel
        for idx in range(dml_combo.count()):
            if dml_combo.itemData(idx) == dml_dev:
                dml_combo.setCurrentIndex(idx)
                break
        dml_combo.setToolTip(i18n.tr("uvr5.tooltip_dml"))
        form.addRow("Adapteur DirectML (GPU):", dml_combo)
        model_edit = QtWidgets.QLineEdit()
        model_edit.setText(model)
        model_edit.setPlaceholderText("C:\\...\\models\\MDX_Net_Models\\MDX23C-8KFFT-InstVoc_HQ.ckpt")
        model_edit.setMinimumWidth(320)
        model_edit.setToolTip(i18n.tr("uvr5.tooltip_model"))
        model_row = QtWidgets.QHBoxLayout()
        model_row.addWidget(model_edit)
        model_browse_btn = QtWidgets.QPushButton(i18n.tr("uvr5.model_browse"))
        def do_browse_model():
            p, _ = QtWidgets.QFileDialog.getOpenFileName(
                dialog,
                i18n.tr("uvr5.model_label"),
                model_edit.text() or "",
                "Modèles (*.ckpt;*.onnx;*.yaml;*.pth);;Tous (*.*)",
            )
            if p:
                model_edit.setText(p)
        model_browse_btn.clicked.connect(do_browse_model)
        model_row.addWidget(model_browse_btn)
        form.addRow(i18n.tr("uvr5.model_label"), model_row)
        layout.addLayout(form)
        grp = QtWidgets.QGroupBox(i18n.tr("uvr5.advanced"))
        adv_layout = QtWidgets.QFormLayout(grp)
        mdx_segment_spin = QtWidgets.QSpinBox()
        mdx_segment_spin.setRange(256, 4096)
        mdx_segment_spin.setValue(min(4096, max(256, mdx_seg)))
        mdx_segment_spin.setToolTip(i18n.tr("uvr5.tooltip_mdx_segment"))
        adv_layout.addRow(i18n.tr("uvr5.mdx_segment_size"), mdx_segment_spin)
        mdxc_overlap_spin = QtWidgets.QSpinBox()
        mdxc_overlap_spin.setRange(2, 50)
        mdxc_overlap_spin.setValue(min(50, max(2, mdxc_ov)))
        mdxc_overlap_spin.setToolTip(i18n.tr("uvr5.tooltip_mdxc_overlap"))
        adv_layout.addRow(i18n.tr("uvr5.mdxc_overlap"), mdxc_overlap_spin)
        mdx_batch_spin = QtWidgets.QSpinBox()
        mdx_batch_spin.setRange(1, 32)
        mdx_batch_spin.setValue(min(32, max(1, mdx_bs)))
        mdx_batch_spin.setToolTip(i18n.tr("uvr5.tooltip_mdx_batch"))
        adv_layout.addRow(i18n.tr("uvr5.mdx_batch_size"), mdx_batch_spin)
        layout.addWidget(grp)
        browse_row = QtWidgets.QHBoxLayout()
        browse_btn = QtWidgets.QPushButton(i18n.tr("uvr5.browse"))
        def do_browse_file():
            p, _ = QtWidgets.QFileDialog.getOpenFileName(
                dialog,
                "audio-separator",
                path_edit.text() or "",
                "Python (*.py);;Executables (*.exe);;All (*.*)" if sys.platform == "win32" else "Python (*.py);;All (*.*)",
            )
            if p:
                path_edit.setText(p)
        browse_btn.clicked.connect(do_browse_file)
        browse_folder_btn = QtWidgets.QPushButton(i18n.tr("uvr5.browse_folder"))
        def do_browse_folder():
            p = QtWidgets.QFileDialog.getExistingDirectory(
                dialog,
                i18n.tr("uvr5.browse_folder"),
                path_edit.text() or "",
            )
            if p:
                path_edit.setText(p)
        browse_folder_btn.clicked.connect(do_browse_folder)
        browse_row.addWidget(browse_btn)
        browse_row.addWidget(browse_folder_btn)
        layout.addLayout(browse_row)
        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            path_value = path_edit.text().strip()
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_UVR5_ENABLED, enabled_cb.isChecked())
            settings.setValue(SETTINGS_UVR5_PATH, path_value)
            settings.setValue(SETTINGS_UVR5_CHUNK_SEC, chunk_spin.value())
            settings.setValue(SETTINGS_UVR5_MODEL, model_edit.text().strip())
            settings.setValue(SETTINGS_UVR5_MDX_SEGMENT_SIZE, mdx_segment_spin.value())
            settings.setValue(SETTINGS_UVR5_MDXC_OVERLAP, mdxc_overlap_spin.value())
            settings.setValue(SETTINGS_UVR5_MDX_BATCH_SIZE, mdx_batch_spin.value())
            settings.setValue(SETTINGS_UVR5_DIRECTML_DEVICE, dml_combo.currentData())
            settings.setValue(SETTINGS_UVR5_STEM, stem_combo.currentText())
            settings.endGroup()
            settings.sync()
            if path_value and uvr5_module.is_uvr_gui_install_folder(path_value):
                QtWidgets.QMessageBox.information(
                    self,
                    i18n.tr("uvr5.gui_only_title"),
                    i18n.tr("uvr5.gui_only_body"),
                )

    def _show_warmth_dialog(self):
        """Fenêtre de configuration de la chaleur vocale (Pedalboard)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        high_shelf_gain = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_GAIN, DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB))
        high_shelf_cutoff = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_CUTOFF, DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ))
        distortion_drive = float(settings.value(SETTINGS_WARMTH_DISTORTION_DRIVE, DEFAULT_WARMTH_DISTORTION_DRIVE_DB))
        compressor_threshold = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_THRESHOLD, DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB))
        compressor_ratio = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_RATIO, DEFAULT_WARMTH_COMPRESSOR_RATIO))
        final_gain = float(settings.value(SETTINGS_WARMTH_FINAL_GAIN, DEFAULT_WARMTH_FINAL_GAIN_DB))
        settings.endGroup()
        
        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("warmth.dialog_title"))
        layout = QtWidgets.QVBoxLayout(dialog)
        
        # Description
        desc_label = QtWidgets.QLabel(i18n.tr("warmth.description"))
        desc_label.setWordWrap(True)
        layout.addWidget(desc_label)
        
        # Presets
        preset_layout = QtWidgets.QHBoxLayout()
        preset_label = QtWidgets.QLabel(i18n.tr("warmth.preset"))
        preset_combo = QtWidgets.QComboBox()
        preset_combo.addItem(i18n.tr("warmth.preset_custom"), "custom")
        preset_combo.addItem(i18n.tr("warmth.preset_subtle"), "subtle")
        preset_combo.addItem(i18n.tr("warmth.preset_standard"), "standard")
        preset_combo.addItem(i18n.tr("warmth.preset_radio"), "radio")
        preset_combo.addItem(i18n.tr("warmth.preset_podcast"), "podcast")
        preset_layout.addWidget(preset_label)
        preset_layout.addWidget(preset_combo)
        layout.addLayout(preset_layout)
        
        # Paramètres
        form = QtWidgets.QFormLayout()
        
        high_shelf_gain_spin = QtWidgets.QDoubleSpinBox()
        high_shelf_gain_spin.setRange(-10.0, 10.0)
        high_shelf_gain_spin.setSingleStep(0.5)
        high_shelf_gain_spin.setValue(high_shelf_gain)
        high_shelf_gain_spin.setSuffix(" dB")
        high_shelf_gain_spin.setToolTip(i18n.tr("warmth.tooltip_high_shelf_gain"))
        form.addRow(i18n.tr("warmth.high_shelf_gain"), high_shelf_gain_spin)
        
        high_shelf_cutoff_spin = QtWidgets.QDoubleSpinBox()
        high_shelf_cutoff_spin.setRange(100.0, 1000.0)
        high_shelf_cutoff_spin.setSingleStep(50.0)
        high_shelf_cutoff_spin.setValue(high_shelf_cutoff)
        high_shelf_cutoff_spin.setSuffix(" Hz")
        high_shelf_cutoff_spin.setToolTip(i18n.tr("warmth.tooltip_high_shelf_cutoff"))
        form.addRow(i18n.tr("warmth.high_shelf_cutoff"), high_shelf_cutoff_spin)
        
        distortion_drive_spin = QtWidgets.QDoubleSpinBox()
        distortion_drive_spin.setRange(0.0, 10.0)
        distortion_drive_spin.setSingleStep(0.5)
        distortion_drive_spin.setValue(distortion_drive)
        distortion_drive_spin.setSuffix(" dB")
        distortion_drive_spin.setToolTip(i18n.tr("warmth.tooltip_distortion"))
        form.addRow(i18n.tr("warmth.distortion_drive"), distortion_drive_spin)
        
        compressor_threshold_spin = QtWidgets.QDoubleSpinBox()
        compressor_threshold_spin.setRange(-30.0, 0.0)
        compressor_threshold_spin.setSingleStep(1.0)
        compressor_threshold_spin.setValue(compressor_threshold)
        compressor_threshold_spin.setSuffix(" dB")
        compressor_threshold_spin.setToolTip(i18n.tr("warmth.tooltip_compressor_threshold"))
        form.addRow(i18n.tr("warmth.compressor_threshold"), compressor_threshold_spin)
        
        compressor_ratio_spin = QtWidgets.QDoubleSpinBox()
        compressor_ratio_spin.setRange(1.0, 10.0)
        compressor_ratio_spin.setSingleStep(0.5)
        compressor_ratio_spin.setValue(compressor_ratio)
        compressor_ratio_spin.setToolTip(i18n.tr("warmth.tooltip_compressor_ratio"))
        form.addRow(i18n.tr("warmth.compressor_ratio"), compressor_ratio_spin)
        
        final_gain_spin = QtWidgets.QDoubleSpinBox()
        final_gain_spin.setRange(-10.0, 10.0)
        final_gain_spin.setSingleStep(0.5)
        final_gain_spin.setValue(final_gain)
        final_gain_spin.setSuffix(" dB")
        final_gain_spin.setToolTip(i18n.tr("warmth.tooltip_final_gain"))
        form.addRow(i18n.tr("warmth.final_gain"), final_gain_spin)
        
        layout.addLayout(form)
        
        # Fonction pour appliquer un preset
        def apply_preset(index):
            preset = preset_combo.itemData(index)
            if preset == "subtle":
                high_shelf_gain_spin.setValue(1.0)
                high_shelf_cutoff_spin.setValue(350.0)
                distortion_drive_spin.setValue(2.0)
                compressor_threshold_spin.setValue(-18.0)
                compressor_ratio_spin.setValue(2.5)
                final_gain_spin.setValue(1.5)
            elif preset == "standard":
                high_shelf_gain_spin.setValue(2.0)
                high_shelf_cutoff_spin.setValue(400.0)
                distortion_drive_spin.setValue(3.0)
                compressor_threshold_spin.setValue(-15.0)
                compressor_ratio_spin.setValue(3.0)
                final_gain_spin.setValue(2.0)
            elif preset == "radio":
                high_shelf_gain_spin.setValue(3.0)
                high_shelf_cutoff_spin.setValue(300.0)
                distortion_drive_spin.setValue(5.0)
                compressor_threshold_spin.setValue(-20.0)
                compressor_ratio_spin.setValue(4.0)
                final_gain_spin.setValue(3.0)
            elif preset == "podcast":
                high_shelf_gain_spin.setValue(1.5)
                high_shelf_cutoff_spin.setValue(450.0)
                distortion_drive_spin.setValue(2.5)
                compressor_threshold_spin.setValue(-18.0)
                compressor_ratio_spin.setValue(2.0)
                final_gain_spin.setValue(1.5)
        
        # Fonction pour passer en mode Custom quand on modifie manuellement
        def set_custom():
            preset_combo.setCurrentIndex(0)  # Custom
        
        preset_combo.currentIndexChanged.connect(apply_preset)
        high_shelf_gain_spin.valueChanged.connect(set_custom)
        high_shelf_cutoff_spin.valueChanged.connect(set_custom)
        distortion_drive_spin.valueChanged.connect(set_custom)
        compressor_threshold_spin.valueChanged.connect(set_custom)
        compressor_ratio_spin.valueChanged.connect(set_custom)
        final_gain_spin.valueChanged.connect(set_custom)
        
        # Boutons
        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)
        
        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_WARMTH_HIGH_SHELF_GAIN, high_shelf_gain_spin.value())
            settings.setValue(SETTINGS_WARMTH_HIGH_SHELF_CUTOFF, high_shelf_cutoff_spin.value())
            settings.setValue(SETTINGS_WARMTH_DISTORTION_DRIVE, distortion_drive_spin.value())
            settings.setValue(SETTINGS_WARMTH_COMPRESSOR_THRESHOLD, compressor_threshold_spin.value())
            settings.setValue(SETTINGS_WARMTH_COMPRESSOR_RATIO, compressor_ratio_spin.value())
            settings.setValue(SETTINGS_WARMTH_FINAL_GAIN, final_gain_spin.value())
            settings.endGroup()
            settings.sync()

    def _show_dpdf_dialog(self):
        """Fenêtre de configuration du nettoyage DPDF (dpdfnet)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        model = str(settings.value(SETTINGS_DPDF_MODEL, DEFAULT_DPDF_MODEL) or DEFAULT_DPDF_MODEL).strip()
        try:
            attn = float(settings.value(SETTINGS_DPDF_ATTN_LIMIT_DB, DEFAULT_DPDF_ATTN_LIMIT_DB))
        except (TypeError, ValueError):
            attn = DEFAULT_DPDF_ATTN_LIMIT_DB
        settings.endGroup()
        if model not in dpdf.AVAILABLE_MODELS:
            model = DEFAULT_DPDF_MODEL

        dialog = QtWidgets.QDialog(self)
        dialog.setWindowTitle(i18n.tr("dpdf.dialog_title"))
        layout = QtWidgets.QVBoxLayout(dialog)

        desc_label = QtWidgets.QLabel(i18n.tr("dpdf.description"))
        desc_label.setWordWrap(True)
        layout.addWidget(desc_label)

        if not dpdf.is_dpdf_available():
            warn = QtWidgets.QLabel(i18n.tr("dpdf.not_installed"))
            warn.setWordWrap(True)
            layout.addWidget(warn)

        form = QtWidgets.QFormLayout()
        model_combo = QtWidgets.QComboBox()
        for mid in dpdf.AVAILABLE_MODELS:
            model_combo.addItem(mid, mid)
        idx = model_combo.findData(model)
        model_combo.setCurrentIndex(idx if idx >= 0 else 0)
        form.addRow(i18n.tr("dpdf.model"), model_combo)

        attn_spin = QtWidgets.QDoubleSpinBox()
        attn_spin.setRange(0.0, 40.0)
        attn_spin.setSingleStep(1.0)
        attn_spin.setValue(attn)
        attn_spin.setSuffix(" dB")
        form.addRow(i18n.tr("dpdf.attn_limit"), attn_spin)
        layout.addLayout(form)

        buttons = QtWidgets.QDialogButtonBox(
            QtWidgets.QDialogButtonBox.StandardButton.Ok | QtWidgets.QDialogButtonBox.StandardButton.Cancel
        )
        buttons.accepted.connect(dialog.accept)
        buttons.rejected.connect(dialog.reject)
        layout.addWidget(buttons)

        if dialog.exec() == QtWidgets.QDialog.DialogCode.Accepted:
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            settings.setValue(SETTINGS_DPDF_MODEL, model_combo.currentData() or DEFAULT_DPDF_MODEL)
            settings.setValue(SETTINGS_DPDF_ATTN_LIMIT_DB, attn_spin.value())
            settings.endGroup()
            settings.sync()

    def _set_theme(self, theme: str):
        if theme == "light" and self.theme_manager.current_theme != "light":
            self.theme_manager.apply_light()
        elif theme == "dark" and self.theme_manager.current_theme != "dark":
            self.theme_manager.apply_dark()
        self._refresh_theme_menu_actions()

    def _apply_palette_to_menu(self):
        """Force les couleurs du menu par stylesheet (la palette seule ne suffit pas sous Windows)."""
        if self.theme_manager.current_theme == "light":
            self._settings_menu.setStyleSheet("""
                QMenu {
                    background-color: #FAFAFA;
                    color: #191919;
                }
                QMenu::item:selected {
                    background-color: #0078D7;
                    color: white;
                }
                QMenu::separator {
                    height: 1px;
                    background-color: #E0E0E0;
                }
            """)
        else:
            self._settings_menu.setStyleSheet("""
                QMenu {
                    background-color: #242424;
                    color: white;
                }
                QMenu::item:selected {
                    background-color: #0078D7;
                    color: white;
                }
                QMenu::separator {
                    height: 1px;
                    background-color: #404040;
                }
            """)

    def _refresh_theme_menu_actions(self):
        is_dark = self.theme_manager.current_theme == "dark"
        self._action_theme_light.setChecked(not is_dark)
        self._action_theme_dark.setCheckable(True)
        self._action_theme_light.setCheckable(True)
        self._action_theme_dark.setChecked(is_dark)

    def _set_language(self, lang: str):
        if lang not in ("fr", "en", "de", "es"):
            return
        i18n.set_language(lang)
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        settings.setValue(i18n.SETTINGS_LANG, lang)
        settings.endGroup()
        self._retranslate_ui()
        self._refresh_language_menu_actions()

    def _refresh_language_menu_actions(self):
        current = i18n.get_language()
        self._action_lang_fr.setChecked(current == "fr")
        self._action_lang_en.setChecked(current == "en")
        self._action_lang_de.setChecked(current == "de")
        self._action_lang_es.setChecked(current == "es")

    def _retranslate_ui(self):
        self.setWindowTitle(i18n.tr("app.title"))
        self._queue_label.setText(i18n.tr("queue.label"))
        self.queue_table.setHorizontalHeaderLabels(
            [
                "",
                i18n.tr("queue.header_file"),
                i18n.tr("queue.header_gain"),
                i18n.tr("queue.header_isolate"),
                i18n.tr("queue.header_transcribe"),
                i18n.tr("queue.header_warmth"),
                i18n.tr("queue.header_dpdf"),
                "",
            ]
        )
        self.add_files_button.setText(i18n.tr("btn.add_files"))
        self._suffix_label.setText(i18n.tr("form.suffix_label"))
        self.suffix_edit.setPlaceholderText(i18n.tr("form.suffix_placeholder"))
        self.keep_flac_checkbox.setText(i18n.tr("form.keep_flac"))
        self._output_dir_label.setText(i18n.tr("form.output_dir"))
        self.output_dir_edit.setPlaceholderText(i18n.tr("form.output_placeholder"))
        self.convert_button.setText(i18n.tr("btn.convert"))
        self.cancel_button.setText(i18n.tr("btn.cancel"))
        self.open_folder_button.setText(i18n.tr("btn.choose"))
        self._settings_button.setToolTip(i18n.tr("settings.tooltip"))
        self._action_theme_light.setText(i18n.tr("theme.light"))
        self._action_theme_dark.setText(i18n.tr("theme.dark"))
        self._action_lang_fr.setText(i18n.tr("lang.fr"))
        self._action_lang_en.setText(i18n.tr("lang.en"))
        self._action_lang_de.setText(i18n.tr("lang.de"))
        self._action_lang_es.setText(i18n.tr("lang.es"))
        self._action_vibe_path.setText(i18n.tr("vibe.path_menu"))
        self._action_vibe_options.setText(i18n.tr("vibe.options_menu"))
        self._action_conv.setText(i18n.tr("conv.menu"))
        self._action_uvr5.setText(i18n.tr("uvr5.menu"))
        self._action_warmth.setText(i18n.tr("warmth.menu"))
        self._action_dpdf.setText(i18n.tr("dpdf.menu"))
        self.transcribe_button.setText(i18n.tr("btn.transcribe"))
        self.transcribe_button.setToolTip(i18n.tr("transcribe.tooltip"))
        self.process_all_button.setText(i18n.tr("btn.process_all"))
        self.process_all_button.setToolTip(i18n.tr("process_all.tooltip"))
        self._update_transcribe_button()
        if not self.progress_bar.isVisible():
            self.progress_bar.setFormat(i18n.tr("progress.waiting"))
        self.player.set_ui_strings(
            no_file=i18n.tr("player.no_file"),
            play=i18n.tr("player.play"),
            pause=i18n.tr("player.pause"),
            stop=i18n.tr("player.stop"),
            preview=i18n.tr("player.preview"),
            return_full=i18n.tr("player.return_full"),
        )

    def _convert_audio(self):
        self._conversion_queue = []
        keep_flac = self.keep_flac_checkbox.isChecked()
        for row in range(self.queue_table.rowCount()):
            item = self._get_queue_item(row)
            if item and item[0].exists():
                isolate = self._get_row_isolate_checked(row)
                apply_warmth = self._get_row_warmth_checked(row)
                apply_dpdf = self._get_row_dpdf_checked(row)
                # Queue: (path, gain, isolate, keep_flac, apply_warmth, apply_dpdf)
                self._conversion_queue.append((item[0], item[1], isolate, keep_flac, apply_warmth, apply_dpdf))
        if not self._conversion_queue:
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("msg.no_file_title"),
                i18n.tr("msg.no_file_body"),
            )
            return

        if not self._ffmpeg_available():
            QtWidgets.QMessageBox.critical(
                self,
                i18n.tr("msg.ffmpeg_missing_title"),
                i18n.tr("msg.ffmpeg_missing_body"),
            )
            return

        self._current_conversion_index = 0
        self._start_next_in_queue()

    def _start_next_in_queue(self):
        if self._current_conversion_index < 0 or self._current_conversion_index >= len(self._conversion_queue):
            self._on_queue_conversion_done()
            return
        input_path, gain, isolate, keep_flac, apply_warmth, apply_dpdf = self._conversion_queue[self._current_conversion_index]
        output_dir = self._output_dir or input_path.parent
        suffix = (self.suffix_edit.text().strip() or "").strip()
        output_path = output_dir / f"{input_path.stem}{suffix}.opus"
        total = len(self._conversion_queue)
        
        # Stocker keep_flac / warmth / dpdf pour l'utiliser après UVR5
        self._conversion_keep_flac = keep_flac
        self._conversion_apply_warmth = apply_warmth
        self._conversion_apply_dpdf = apply_dpdf
        self._conversion_dpdf_applied = False

        if isolate:
            settings = QtCore.QSettings()
            uvr_path = uvr5_module.get_uvr5_path(settings)
            if uvr5_module.uvr5_available(uvr_path):
                self._set_row_progress(self._current_conversion_index, -1)
                
                # Récupérer d'abord tous les paramètres UVR5
                chunk_sec = uvr5_module.get_uvr5_chunk_sec(settings)
                model = uvr5_module.get_uvr5_model(settings)
                settings = QtCore.QSettings()
                mdx_seg = uvr5_module.get_uvr5_mdx_segment_size(settings)
                mdxc_ov = uvr5_module.get_uvr5_mdxc_overlap(settings)
                mdx_bs = uvr5_module.get_uvr5_mdx_batch_size(settings)
                dml_device = uvr5_module.get_uvr5_directml_device(settings)
                stem = uvr5_module.get_uvr5_stem(settings)
                
                self.log_output.appendPlainText(f"UVR5: extraction de '{stem}' pour {input_path.name}")
                self._cancel_requested = False
                self.progress_bar.setVisible(True)
                self.progress_bar.setRange(0, 0)
                self.progress_bar.setFormat("UVR5…")
                self.convert_button.setEnabled(False)
                self.transcribe_button.setEnabled(False)
                self.process_all_button.setEnabled(False)
                self.open_folder_button.setEnabled(False)
                self.add_files_button.setEnabled(False)
                self.queue_table.setEnabled(False)
                self.cancel_button.setEnabled(True)
                self._conversion_after_uvr5_output_path = output_path
                self._conversion_after_uvr5_gain = gain
                self._uvr5_output_dir = Path(tempfile.mkdtemp(prefix="compressspeak_uvr5_"))
                cmd = uvr5_module.build_uvr5_command(
                    uvr_path, input_path, self._uvr5_output_dir, chunk_sec, model,
                    mdx_segment_size=mdx_seg, mdxc_overlap=mdxc_ov, mdx_batch_size=mdx_bs,
                    stem=stem,
                )
                if not cmd:
                    if uvr5_module.is_uvr_gui_install_folder(uvr_path):
                        self.log_output.appendPlainText(i18n.tr("uvr5.gui_only_body").split("\n\n")[0])
                        QtWidgets.QMessageBox.information(
                            self,
                            i18n.tr("uvr5.gui_only_title"),
                            i18n.tr("uvr5.gui_only_body"),
                        )
                    else:
                        self.log_output.appendPlainText(i18n.tr("uvr5.not_found"))
                    self._current_conversion_index += 1
                    self._cleanup_ffmpeg_state()
                    QtCore.QTimer.singleShot(0, self._start_next_in_queue)
                    return
                worker_script = Path(__file__).parent / "uvr5_worker.py"
                model_dir, model_name = "", ""
                if model:
                    from uvr5 import _parse_model_path
                    model_dir, model_name = _parse_model_path(model)
                model_dir = model_dir or "/tmp/audio-separator-models/"
                model_name = model_name or "model_bs_roformer_ep_317_sdr_12.9755.ckpt"
                # Pré-conversion m4a/mp4 → WAV : évite un bug d'audio-separator (muxer m4a vers fichier temp sans extension)
                _m4a_extensions = (".m4a", ".mp4", ".3gp", ".3g2", ".mj2")
                need_prep = input_path.suffix.lower() in _m4a_extensions
                if need_prep:
                    wav_path = self._uvr5_output_dir / "source.wav"
                    if uvr5_module.is_uvr5_default_path(uvr_path) and worker_script.exists():
                        self._uvr5_cmd_after_prep = [
                            sys.executable, str(worker_script),
                            str(wav_path), str(self._uvr5_output_dir),
                            str(chunk_sec), model_dir, model_name,
                            str(mdx_seg), str(mdxc_ov), str(mdx_bs),
                            str(dml_device), stem,
                        ]
                    else:
                        self._uvr5_cmd_after_prep = uvr5_module.build_uvr5_command(
                            uvr_path, wav_path, self._uvr5_output_dir, chunk_sec, model,
                            mdx_segment_size=mdx_seg, mdxc_overlap=mdxc_ov, mdx_batch_size=mdx_bs,
                            stem=stem,
                        )
                    if self._uvr5_cmd_after_prep:
                        self.log_output.appendPlainText("UVR5: conversion préalable m4a/mp4 → WAV...")
                        prep_cmd = ffmpeg.build_m4a_to_wav_command(input_path, wav_path)
                        self._uvr5_prep_ffmpeg_process = QtCore.QProcess(self)
                        self._uvr5_prep_ffmpeg_process.setProgram(prep_cmd[0])
                        self._uvr5_prep_ffmpeg_process.setArguments(prep_cmd[1:])
                        self._uvr5_prep_ffmpeg_process.finished.connect(self._on_uvr5_prep_ffmpeg_finished)
                        self._uvr5_prep_ffmpeg_process.start()
                    else:
                        self._uvr5_cmd_after_prep = None
                    return
                # Stratégie : utiliser un script worker en processus séparé pour DirectML sans gel de l'UI
                if uvr5_module.is_uvr5_default_path(uvr_path):
                    if worker_script.exists():
                        # Mode WORKER PROCESS avec DirectML (GPU AMD/Intel)
                        self.log_output.appendPlainText("=== UVR5: Mode WORKER PROCESS avec DirectML (GPU AMD) ===")
                        python_exe = sys.executable
                        cmd = [
                            python_exe, str(worker_script),
                            str(input_path), str(self._uvr5_output_dir),
                            str(chunk_sec), model_dir, model_name,
                            str(mdx_seg), str(mdxc_ov), str(mdx_bs),
                            str(dml_device),  # Index de l'adapteur DirectML
                            stem  # Stem à extraire (Vocals, Instrumental, etc.)
                        ]
                        
                        self._uvr5_process = QtCore.QProcess(self)
                        env = QtCore.QProcessEnvironment.systemEnvironment()
                        env.insert("PYTHONUNBUFFERED", "1")
                        env.insert("TQDM_DISABLE", "1")
                        self._uvr5_process.setProcessEnvironment(env)
                        self._uvr5_process.setProgram(cmd[0])
                        self._uvr5_process.setArguments(cmd[1:])
                        self._uvr5_process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
                        self._uvr5_process.readyReadStandardError.connect(self._on_uvr5_output)
                        self._uvr5_process.readyReadStandardOutput.connect(self._on_uvr5_output)
                        self._uvr5_process.finished.connect(self._on_uvr5_finished)
                        self._uvr5_process.start()
                        if not self._uvr5_process.waitForStarted(10000):
                            self.log_output.appendPlainText("UVR5: Impossible de démarrer le worker")
                            self._uvr5_process = None
                            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
                            self._uvr5_output_dir = None
                            self._current_conversion_index += 1
                            self._cleanup_ffmpeg_state()
                            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
                        else:
                            # Démarrer un timer pour afficher un message de patience toutes les 30 secondes
                            self._uvr5_heartbeat_timer = QtCore.QTimer(self)
                            self._uvr5_heartbeat_timer.timeout.connect(self._on_uvr5_heartbeat)
                            self._uvr5_heartbeat_timer.start(30000)  # 30 secondes
                        return
                
                # Fallback : utiliser audio-separator en ligne de commande (sans DirectML)
                self.log_output.appendPlainText("UVR5 (subprocess sans DirectML): " + " ".join(cmd))
                self._uvr5_process = QtCore.QProcess(self)
                env = QtCore.QProcessEnvironment.systemEnvironment()
                env.insert("TERM", "dumb")
                env.insert("TQDM_DISABLE", "1")
                self._uvr5_process.setProcessEnvironment(env)
                self._uvr5_process.setProgram(cmd[0])
                self._uvr5_process.setArguments(cmd[1:])
                self._uvr5_process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
                self._uvr5_process.readyReadStandardError.connect(self._on_uvr5_output)
                self._uvr5_process.readyReadStandardOutput.connect(self._on_uvr5_output)
                self._uvr5_process.finished.connect(self._on_uvr5_finished)
                self._uvr5_process.start()
                if not self._uvr5_process.waitForStarted(10000):
                    self.log_output.appendPlainText(i18n.tr("uvr5.not_found"))
                    self._uvr5_process = None
                    if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                        shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
                    self._uvr5_output_dir = None
                    self._current_conversion_index += 1
                    self._cleanup_ffmpeg_state()
                    QtCore.QTimer.singleShot(0, self._start_next_in_queue)
                return
            else:
                self.log_output.appendPlainText(i18n.tr("uvr5.not_found"))

        # Sans isolation : DPDF optionnel puis FFmpeg
        if self._dpdf_temp_dir and self._dpdf_temp_dir.exists():
            shutil.rmtree(self._dpdf_temp_dir, ignore_errors=True)
        self._dpdf_temp_dir = None
        source_for_ffmpeg = input_path
        if self._conversion_apply_dpdf:
            self._dpdf_temp_dir = Path(tempfile.mkdtemp(prefix="compressspeak_dpdf_"))
            source_for_ffmpeg = self._apply_dpdf_step(input_path, self._dpdf_temp_dir)
        params = self._get_conversion_params()
        if self._conversion_dpdf_applied:
            params = {**params, "skip_afftdn": True}
        command = ffmpeg.build_convert_command(source_for_ffmpeg, output_path, gain, params)
        self._set_row_progress(self._current_conversion_index, -1)
        self.log_output.appendPlainText(f"[{self._current_conversion_index + 1}/{total}] {input_path.name} → {output_path.name}")
        self.log_output.appendPlainText("FFmpeg: " + " ".join(command))
        self._current_duration_sec = None
        if self.current_file == input_path:
            self._current_duration_sec = self.player.duration_seconds()
        self._start_ffmpeg(command, output_path)

    def _on_uvr5_heartbeat(self):
        """Affiche un message périodique pour indiquer que le traitement UVR5 continue."""
        if self._uvr5_process and self._uvr5_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            self.log_output.appendPlainText("UVR5: Traitement en cours (utilisation GPU)...")

    def _on_uvr5_prep_ffmpeg_finished(self, exit_code: int, exit_status: QtCore.QProcess.ExitStatus):
        """Après conversion m4a→WAV, lance le worker ou la CLI UVR5."""
        prep_proc = self._uvr5_prep_ffmpeg_process
        self._uvr5_prep_ffmpeg_process = None
        cmd_after = self._uvr5_cmd_after_prep
        self._uvr5_cmd_after_prep = None
        if prep_proc:
            prep_proc.deleteLater()
        if self._cancel_requested or not cmd_after:
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        if exit_status != QtCore.QProcess.ExitStatus.NormalExit or exit_code != 0:
            self.log_output.appendPlainText("UVR5: échec de la conversion préalable m4a → WAV.")
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        self.log_output.appendPlainText("UVR5: WAV prêt, lancement de l'extraction...")
        self._uvr5_process = QtCore.QProcess(self)
        env = QtCore.QProcessEnvironment.systemEnvironment()
        env.insert("PYTHONUNBUFFERED", "1")
        env.insert("TQDM_DISABLE", "1")
        self._uvr5_process.setProcessEnvironment(env)
        self._uvr5_process.setProgram(cmd_after[0])
        self._uvr5_process.setArguments(cmd_after[1:])
        self._uvr5_process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
        self._uvr5_process.readyReadStandardError.connect(self._on_uvr5_output)
        self._uvr5_process.readyReadStandardOutput.connect(self._on_uvr5_output)
        self._uvr5_process.finished.connect(self._on_uvr5_finished)
        self._uvr5_process.start()
        if not self._uvr5_process.waitForStarted(10000):
            self.log_output.appendPlainText("UVR5: Impossible de démarrer le worker")
            self._uvr5_process = None
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        is_worker = "uvr5_worker" in str(cmd_after[1])
        if is_worker:
            self._uvr5_heartbeat_timer = QtCore.QTimer(self)
            self._uvr5_heartbeat_timer.timeout.connect(self._on_uvr5_heartbeat)
            self._uvr5_heartbeat_timer.start(30000)
    
    def _on_uvr5_output(self):
        if not self._uvr5_process:
            return
        
        current_time = time.time()
        
        for reader in (self._uvr5_process.readAllStandardOutput, self._uvr5_process.readAllStandardError):
            data = reader()
            if data:
                text = bytes(data).decode(errors="ignore").strip()
                if text:
                    lines = text.split('\n')
                    for line in lines:
                        line = line.strip()
                        if not line:
                            continue
                        
                        # Parser la sortie du script worker
                        if line.startswith("STEM_OUTPUT:"):
                            stem_path_str = line[len("STEM_OUTPUT:"):].strip()
                            self._uvr5_worker_vocals_path = Path(stem_path_str)
                            continue
                        elif line.startswith("VOCALS_OUTPUT:"):
                            stem_path_str = line[len("VOCALS_OUTPUT:"):].strip()
                            self._uvr5_worker_vocals_path = Path(stem_path_str)
                            continue
                        
                        # Messages critiques toujours affichés
                        critical_keywords = [
                            ">>>", "DirectML", "DmlExecutionProvider", "acceleration",
                            "Error", "Erreur", "Failed", "WARNING", "VOCALS_OUTPUT",
                            "Splitting", "chunks"  # Pour voir le découpage audio
                        ]
                        is_critical = any(kw in line for kw in critical_keywords)
                        
                        # Messages importants affichés avec throttling
                        important_keywords = [
                            "Processing chunk", "Chargement", "complete", "terminée",
                            "Initialisation", "Séparation"
                        ]
                        is_important = any(kw in line for kw in important_keywords)
                        
                        # Filtrer les lignes tqdm et les logs DEBUG verbeux
                        is_tqdm = all(c in line for c in ['|', '%', '/', '[']) and ('it/s' in line or '<' in line)
                        is_debug_noise = line.startswith("DEBUG -") and not is_critical
                        
                        # Afficher avec throttling intelligent
                        if is_critical:
                            # Messages critiques toujours affichés immédiatement
                            self.log_output.appendPlainText(line)
                        elif is_important and (current_time - self._uvr5_last_log_time >= 2.0):
                            # Messages importants avec throttling de 2 secondes
                            self.log_output.appendPlainText(line)
                            self._uvr5_last_log_time = current_time
                        elif not is_tqdm and not is_debug_noise and (current_time - self._uvr5_last_log_time >= 5.0):
                            # Autres messages avec throttling de 5 secondes
                            self.log_output.appendPlainText(line)
                            self._uvr5_last_log_time = current_time

    def _on_uvr5_inprocess_finished(self, vocals_path):
        """Appelé quand run_uvr5_vocals_inprocess a terminé (thread). Même logique que _on_uvr5_finished."""
        out_dir = self._uvr5_output_dir
        output_path = self._conversion_after_uvr5_output_path
        gain = self._conversion_after_uvr5_gain
        if getattr(self, "_uvr5_thread", None):
            self._uvr5_thread.quit()
            self._uvr5_thread.wait(5000)
            self._uvr5_thread = None
        self._uvr5_worker = None
        if self._cancel_requested:
            if out_dir and out_dir.exists():
                shutil.rmtree(out_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        if not vocals_path:
            if output_path:
                self.log_output.appendPlainText(i18n.tr("log.uvr5_failed", name=output_path.name))
            if out_dir and out_dir.exists():
                shutil.rmtree(out_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        self.log_output.appendPlainText(i18n.tr("log.uvr5_done", path=str(vocals_path)))
        # DPDF puis chaleur (même ordre que le chemin worker)
        vocals_path = self._apply_dpdf_step(vocals_path, out_dir)
        if self._conversion_apply_warmth and warmth.is_pedalboard_available() and out_dir:
            self.log_output.appendPlainText("Application de la chaleur vocale...")
            warmth_output = out_dir / f"{Path(vocals_path).stem}_warm{Path(vocals_path).suffix}"
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            high_shelf_gain = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_GAIN, DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB))
            high_shelf_cutoff = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_CUTOFF, DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ))
            distortion_drive = float(settings.value(SETTINGS_WARMTH_DISTORTION_DRIVE, DEFAULT_WARMTH_DISTORTION_DRIVE_DB))
            compressor_threshold = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_THRESHOLD, DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB))
            compressor_ratio = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_RATIO, DEFAULT_WARMTH_COMPRESSOR_RATIO))
            final_gain = float(settings.value(SETTINGS_WARMTH_FINAL_GAIN, DEFAULT_WARMTH_FINAL_GAIN_DB))
            settings.endGroup()
            if warmth.apply_warmth(
                vocals_path,
                warmth_output,
                high_shelf_gain_db=high_shelf_gain,
                high_shelf_cutoff_hz=high_shelf_cutoff,
                distortion_drive_db=distortion_drive,
                compressor_threshold_db=compressor_threshold,
                compressor_ratio=compressor_ratio,
                final_gain_db=final_gain,
            ):
                vocals_path = warmth_output
        params = self._get_conversion_params()
        if self._conversion_dpdf_applied:
            params = {**params, "skip_afftdn": True}
        command = ffmpeg.build_convert_command(vocals_path, output_path, gain, params)
        self._current_duration_sec = None
        self._start_ffmpeg(command, output_path)

    def _on_uvr5_finished(self, exit_code: int, exit_status: QtCore.QProcess.ExitStatus):
        # Arrêter le timer heartbeat
        if self._uvr5_heartbeat_timer:
            self._uvr5_heartbeat_timer.stop()
            self._uvr5_heartbeat_timer = None
        
        process = self._uvr5_process
        self._uvr5_process = None
        out_dir = self._uvr5_output_dir
        output_path = self._conversion_after_uvr5_output_path
        gain = self._conversion_after_uvr5_gain
        if process:
            process.deleteLater()
        
        # Utiliser le chemin parsé du worker si disponible
        vocals_path = self._uvr5_worker_vocals_path
        self._uvr5_worker_vocals_path = None
        
        # Sinon, chercher dans le dossier de sortie
        if not vocals_path and out_dir and out_dir.exists():
            # Chercher n'importe quel fichier audio (le stem peut être Vocals, Instrumental, etc.)
            for f in out_dir.iterdir():
                if f.suffix.lower() in (".wav", ".flac", ".mp3", ".ogg"):
                    vocals_path = f
                    break
        
        if not vocals_path or exit_status != QtCore.QProcess.ExitStatus.NormalExit or exit_code != 0:
            if output_path:
                self.log_output.appendPlainText(i18n.tr("log.uvr5_failed", name=output_path.name))
            if out_dir and out_dir.exists():
                shutil.rmtree(out_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        self.log_output.appendPlainText(i18n.tr("log.uvr5_done", path=str(vocals_path)))

        # DPDF avant chaleur
        vocals_path = self._apply_dpdf_step(vocals_path, out_dir)

        # Appliquer la chaleur vocale si demandé
        if self._conversion_apply_warmth and warmth.is_pedalboard_available():
            self.log_output.appendPlainText("Application de la chaleur vocale...")
            warmth_output = out_dir / f"{Path(vocals_path).stem}_warm{Path(vocals_path).suffix}"

            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            high_shelf_gain = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_GAIN, DEFAULT_WARMTH_HIGH_SHELF_GAIN_DB))
            high_shelf_cutoff = float(settings.value(SETTINGS_WARMTH_HIGH_SHELF_CUTOFF, DEFAULT_WARMTH_HIGH_SHELF_CUTOFF_HZ))
            distortion_drive = float(settings.value(SETTINGS_WARMTH_DISTORTION_DRIVE, DEFAULT_WARMTH_DISTORTION_DRIVE_DB))
            compressor_threshold = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_THRESHOLD, DEFAULT_WARMTH_COMPRESSOR_THRESHOLD_DB))
            compressor_ratio = float(settings.value(SETTINGS_WARMTH_COMPRESSOR_RATIO, DEFAULT_WARMTH_COMPRESSOR_RATIO))
            final_gain = float(settings.value(SETTINGS_WARMTH_FINAL_GAIN, DEFAULT_WARMTH_FINAL_GAIN_DB))
            settings.endGroup()

            if warmth.apply_warmth(
                vocals_path,
                warmth_output,
                high_shelf_gain_db=high_shelf_gain,
                high_shelf_cutoff_hz=high_shelf_cutoff,
                distortion_drive_db=distortion_drive,
                compressor_threshold_db=compressor_threshold,
                compressor_ratio=compressor_ratio,
                final_gain_db=final_gain,
            ):
                self.log_output.appendPlainText(f"Chaleur appliquée: {warmth_output.name}")
                vocals_path = warmth_output
            else:
                self.log_output.appendPlainText("ATTENTION: Échec de l'application de la chaleur, utilisation du fichier d'origine")
        elif self._conversion_apply_warmth:
            self.log_output.appendPlainText("ATTENTION: Pedalboard non installé, chaleur ignorée")

        params = self._get_conversion_params()
        if self._conversion_dpdf_applied:
            params = {**params, "skip_afftdn": True}

        command = ffmpeg.build_convert_command(vocals_path, output_path, gain, params)
        self._current_duration_sec = None
        self._start_ffmpeg(command, output_path)

    def _start_ffmpeg(self, command: list[str], output_path: Path):
        if self._ffmpeg_process:
            QtWidgets.QMessageBox.warning(
                self,
                i18n.tr("msg.convert_busy_title"),
                i18n.tr("msg.convert_busy_body"),
            )
            return

        self._cancel_requested = False
        self.progress_bar.setVisible(True)
        if self._current_duration_sec:
            self.progress_bar.setRange(0, 100)
            self.progress_bar.setValue(0)
            self.progress_bar.setFormat(i18n.tr("progress.preparing"))
        else:
            self.progress_bar.setRange(0, 0)
            self.progress_bar.setFormat(i18n.tr("progress.unknown_duration"))
        self.convert_button.setEnabled(False)
        self.transcribe_button.setEnabled(False)
        self.process_all_button.setEnabled(False)
        self.open_folder_button.setEnabled(False)
        self.add_files_button.setEnabled(False)
        self.queue_table.setEnabled(False)
        self.cancel_button.setEnabled(True)

        self._ffmpeg_process = QtCore.QProcess(self)
        self._ffmpeg_process.setProgram(command[0])
        self._ffmpeg_process.setArguments(command[1:])
        self._ffmpeg_process.setProcessChannelMode(QtCore.QProcess.ProcessChannelMode.SeparateChannels)
        self._ffmpeg_process.readyReadStandardError.connect(lambda: self._handle_ffmpeg_output(output_path))
        self._ffmpeg_process.readyReadStandardOutput.connect(lambda: self._handle_ffmpeg_output(output_path))
        self._ffmpeg_process.finished.connect(lambda code, status: self._ffmpeg_finished(code, status, output_path))
        self._ffmpeg_process.start()

        if not self._ffmpeg_process.waitForStarted(3000):
            self.log_output.appendPlainText(i18n.tr("log.ffmpeg_start_failed"))
            QtWidgets.QMessageBox.critical(self, i18n.tr("msg.error_title"), i18n.tr("msg.ffmpeg_start_failed"))
            self._cleanup_ffmpeg_state()
            return

        self.log_output.appendPlainText(i18n.tr("log.conversion_started"))
        self.progress_bar.setFormat("0 %")

    def _handle_ffmpeg_output(self, output_path: Path):
        if not self._ffmpeg_process:
            return

        for channel, reader in (
            ("STDOUT", self._ffmpeg_process.readAllStandardOutput),
            ("STDERR", self._ffmpeg_process.readAllStandardError),
        ):
            data = reader()
            if not data:
                continue
            text = bytes(data).decode(errors="ignore")
            if text.strip():
                self.log_output.appendPlainText(text.strip())
            self._update_progress_from_output(text)

    def _update_progress_from_output(self, text: str):
        progress = ffmpeg.parse_progress_from_output(text, self._current_duration_sec or 0)
        if progress is None:
            return
        self.progress_bar.setValue(progress)
        self.progress_bar.setFormat(f"{progress} %")
        if 0 <= self._current_conversion_index < self.queue_table.rowCount():
            self._set_row_progress(self._current_conversion_index, progress)

    def _ffmpeg_finished(self, exit_code: int, exit_status: QtCore.QProcess.ExitStatus, output_path: Path):
        success = exit_status == QtCore.QProcess.ExitStatus.NormalExit and exit_code == 0

        if success and not self._cancel_requested:
            # Copier le FLAC si l'option "Conserver FLAC" est cochée
            if self._conversion_keep_flac and self._uvr5_output_dir and self._uvr5_output_dir.exists():
                try:
                    # Chercher le fichier FLAC dans le dossier temporaire
                    for f in self._uvr5_output_dir.iterdir():
                        if f.suffix.lower() == ".flac":
                            # Copier vers le même dossier que l'Opus
                            dest = output_path.parent / f.name
                            shutil.copy2(f, dest)
                            self.log_output.appendPlainText(f"FLAC conservé: {dest}")
                            break
                except Exception as e:
                    self.log_output.appendPlainText(f"Erreur lors de la copie du FLAC: {e}")
            
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._cleanup_dpdf_temp()
            self._set_row_progress(self._current_conversion_index, 100)
            self.progress_bar.setRange(0, 100)
            self.progress_bar.setValue(100)
            total = len(self._conversion_queue)
            idx = self._current_conversion_index + 1
            self.progress_bar.setFormat(f"{i18n.tr('msg.done_title')} ({idx}/{total})")
            self.log_output.appendPlainText(i18n.tr("log.conversion_done", path=str(output_path)))
            self._current_conversion_index += 1
            if self._current_conversion_index < len(self._conversion_queue):
                self._ffmpeg_process.deleteLater()
                self._ffmpeg_process = None
                QtCore.QTimer.singleShot(100, self._start_next_in_queue)
                return
        elif self._cancel_requested:
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._cleanup_dpdf_temp()
            self.progress_bar.setRange(0, 100)
            self.progress_bar.setValue(0)
            self.progress_bar.setFormat(i18n.tr("progress.cancelled"))
            self.log_output.appendPlainText(i18n.tr("log.conversion_cancelled"))
            self._on_queue_conversion_done(success=False)
            return
        else:
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._cleanup_dpdf_temp()
            self.progress_bar.setFormat(i18n.tr("progress.failed"))
            self.log_output.appendPlainText(i18n.tr("log.conversion_failed"))
            QtWidgets.QMessageBox.critical(
                self,
                i18n.tr("msg.ffmpeg_error_title"),
                i18n.tr("msg.ffmpeg_error_body"),
            )
            self._on_queue_conversion_done(success=False)
            return

        self._on_queue_conversion_done(success=True)

    def _on_queue_conversion_done(self, success: bool = False):
        for row in range(self.queue_table.rowCount()):
            self._set_row_progress(row, -2)
        total = len(self._conversion_queue) if self._conversion_queue else 0
        
        # Si on est en mode "Tout traiter" et que la conversion a réussi
        if self._process_all_mode and success and self._process_all_transcribe_after:
            self._cleanup_ffmpeg_state()
            self._process_all_mode = False
            self._process_all_transcribe_after = False
            self.log_output.appendPlainText("=== Conversion terminée, début de la transcription ===")
            # Lancer la transcription après un court délai
            QtCore.QTimer.singleShot(500, self._transcribe_current)
            return
        
        # Sinon, comportement normal
        self._process_all_mode = False
        self._process_all_transcribe_after = False
        
        if success and total > 1:
            QtWidgets.QMessageBox.information(
                self,
                i18n.tr("msg.done_title"),
                i18n.tr("msg.done_multi", total=total),
            )
        elif success and total == 1:
            input_path, gain, isolate, keep_flac, apply_warmth, apply_dpdf = self._conversion_queue[0]
            output_dir = self._output_dir or input_path.parent
            suffix = (self.suffix_edit.text().strip() or "").strip()
            output_path = output_dir / f"{input_path.stem}{suffix}.opus"
            QtWidgets.QMessageBox.information(
                self,
                i18n.tr("msg.done_title"),
                i18n.tr("msg.done_single", path=str(output_path)),
            )
        self._cleanup_ffmpeg_state()

    def _cancel_conversion(self, force: bool = False):
        if getattr(self, "_uvr5_thread", None) and self._uvr5_thread.isRunning():
            self._cancel_requested = True
            self.log_output.appendPlainText(i18n.tr("log.cancel_requested"))
            return
        if self._uvr5_prep_ffmpeg_process and self._uvr5_prep_ffmpeg_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            self._cancel_requested = True
            self.log_output.appendPlainText(i18n.tr("log.cancel_requested"))
            self._uvr5_prep_ffmpeg_process.finished.disconnect()
            self._uvr5_prep_ffmpeg_process.terminate()
            if not self._uvr5_prep_ffmpeg_process.waitForFinished(3000):
                self._uvr5_prep_ffmpeg_process.kill()
            self._uvr5_prep_ffmpeg_process.deleteLater()
            self._uvr5_prep_ffmpeg_process = None
            self._uvr5_cmd_after_prep = None
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._current_conversion_index += 1
            self._cleanup_ffmpeg_state()
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        if self._uvr5_process and self._uvr5_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            # Arrêter le timer heartbeat
            if self._uvr5_heartbeat_timer:
                self._uvr5_heartbeat_timer.stop()
                self._uvr5_heartbeat_timer = None
            
            self._cancel_requested = True
            self.log_output.appendPlainText(i18n.tr("log.cancel_requested"))
            self._uvr5_process.terminate()
            if not self._uvr5_process.waitForFinished(5000):
                self._uvr5_process.kill()
                self._uvr5_process.waitForFinished(2000)
            if self._uvr5_process:
                self._uvr5_process.deleteLater()
            self._uvr5_process = None
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
            self._uvr5_output_dir = None
            self._cleanup_ffmpeg_state()
            self._current_conversion_index += 1
            QtCore.QTimer.singleShot(0, self._start_next_in_queue)
            return
        if not self._ffmpeg_process or self._ffmpeg_process.state() == QtCore.QProcess.ProcessState.NotRunning:
            return

        self._cancel_requested = True
        # Annuler le mode "Tout traiter" si actif
        if self._process_all_mode:
            self._process_all_mode = False
            self._process_all_transcribe_after = False
            self.log_output.appendPlainText("=== Traitement complet annulé ===")
        self.log_output.appendPlainText(i18n.tr("log.cancel_requested"))
        self._ffmpeg_process.terminate()
        if force or not self._ffmpeg_process.waitForFinished(2000):
            self._ffmpeg_process.kill()
            self._ffmpeg_process.waitForFinished(2000)

    def _cleanup_dpdf_temp(self):
        if self._dpdf_temp_dir and self._dpdf_temp_dir.exists():
            shutil.rmtree(self._dpdf_temp_dir, ignore_errors=True)
        self._dpdf_temp_dir = None

    def _cleanup_ffmpeg_state(self):
        if self._ffmpeg_process:
            self._ffmpeg_process.deleteLater()
            self._ffmpeg_process = None
        self._cleanup_dpdf_temp()
        self.convert_button.setEnabled(True)
        self.open_folder_button.setEnabled(True)
        self.add_files_button.setEnabled(True)
        self.queue_table.setEnabled(True)
        self.cancel_button.setEnabled(False)
        self._cancel_requested = False
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.progress_bar.setFormat(i18n.tr("progress.waiting"))
        self.progress_bar.setVisible(False)
        self._update_transcribe_button()

    def _load_form_settings(self):
        """Charge les réglages du formulaire depuis QSettings (au démarrage)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        suffix_val = (settings.value(SETTINGS_FORM_CLEAN_SUFFIX) or "").strip()
        if isinstance(suffix_val, bool):
            suffix_val = "_clean" if suffix_val else ""
        self.suffix_edit.setText(suffix_val if suffix_val else "")
        self.keep_flac_checkbox.setChecked(
            bool(settings.value(SETTINGS_FORM_KEEP_FLAC, False))
        )
        out_dir = (settings.value(SETTINGS_OUTPUT_DIR) or "").strip()
        if out_dir:
            self.output_dir_edit.setText(out_dir)
        settings.endGroup()

    def _save_form_settings(self):
        """Enregistre les réglages du formulaire dans QSettings (à la fermeture)."""
        settings = QtCore.QSettings()
        settings.beginGroup(i18n.SETTINGS_GROUP)
        settings.setValue(SETTINGS_FORM_CLEAN_SUFFIX, self.suffix_edit.text().strip())
        settings.setValue(SETTINGS_FORM_KEEP_FLAC, self.keep_flac_checkbox.isChecked())
        settings.setValue(SETTINGS_OUTPUT_DIR, self.output_dir_edit.text().strip())
        settings.endGroup()

    def closeEvent(self, event: QtGui.QCloseEvent):
        self._save_form_settings()
        if self._ffmpeg_process and self._ffmpeg_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            self._cancel_conversion(force=True)
        if self._uvr5_process and self._uvr5_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            self._uvr5_process.kill()
            self._uvr5_process.waitForFinished(2000)
            if self._uvr5_output_dir and self._uvr5_output_dir.exists():
                shutil.rmtree(self._uvr5_output_dir, ignore_errors=True)
        self._cleanup_dpdf_temp()
        if self._vibe_process and self._vibe_process.state() != QtCore.QProcess.ProcessState.NotRunning:
            self._vibe_process.kill()
            self._vibe_process.waitForFinished(2000)
        self._cancel_preview_process(force=True)
        try:
            shutil.rmtree(self._preview_dir, ignore_errors=True)
        except Exception:
            pass
        super().closeEvent(event)

    def _ffmpeg_available(self) -> bool:
        return ffmpeg.ffmpeg_available()

    def _check_uvr5_availability(self):
        """Vérifie si UVR5 est disponible et met à jour le cache."""
        try:
            settings = QtCore.QSettings()
            settings.beginGroup(i18n.SETTINGS_GROUP)
            uvr_path = str(settings.value(uvr5_module.SETTINGS_UVR5_PATH, "")).strip()
            settings.endGroup()
            self._uvr5_available_cached = uvr5_module.uvr5_available(uvr_path)
        except Exception:
            self._uvr5_available_cached = False
        self._update_transcribe_checkboxes()
    
    def _vibe_available(self) -> bool:
        """Vérifie si le CLI de transcription (Vibe/sona) est disponible. Résultat mis en cache."""
        if self._vibe_available_cached is not None:
            return self._vibe_available_cached
        exe = self._get_vibe_path()
        self._vibe_available_cached = vibe.check_vibe_available(exe)
        return self._vibe_available_cached

    def _vibe_uses_sona_syntax(self) -> bool:
        """True si le CLI attend model puis audio en positionnel (ex. sona). Résultat mis en cache."""
        if self._vibe_sona_syntax_cached is not None:
            return self._vibe_sona_syntax_cached
        exe = self._get_vibe_path()
        log_cb = (lambda msg: self.log_output.appendPlainText(msg)) if self.log_output else None
        self._vibe_sona_syntax_cached = vibe.check_sona_syntax(exe, log_callback=log_cb)
        return self._vibe_sona_syntax_cached

