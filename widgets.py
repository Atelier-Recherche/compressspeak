# -*- coding: utf-8 -*-
"""Widgets réutilisables pour l'interface Compress Speak."""

from pathlib import Path

from PyQt6 import QtCore, QtGui, QtWidgets, QtMultimedia

import i18n
from config import AUDIO_EXTENSIONS, is_audio


class DropContainer(QtWidgets.QFrame):
    """Conteneur qui accepte le glisser-déposer de fichiers (contourne les limitations du QTableWidget)."""
    files_dropped = QtCore.pyqtSignal(list)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setAcceptDrops(True)
        self.setFrameShape(QtWidgets.QFrame.Shape.StyledPanel)
        self.setFrameShadow(QtWidgets.QFrame.Shadow.Sunken)

    def register_child(self, child: QtWidgets.QWidget):
        """Enregistre un enfant pour qu'il transmette les événements de drop au conteneur."""
        child.setAcceptDrops(True)
        child.installEventFilter(self)

    def eventFilter(self, obj, event):
        t = event.type()
        if t == QtCore.QEvent.Type.DragEnter:
            if self._accept_drag(event):
                event.setDropAction(QtCore.Qt.DropAction.CopyAction)
                event.accept()
                return True
            return False
        if t == QtCore.QEvent.Type.DragMove:
            if self._accept_drag(event):
                event.setDropAction(QtCore.Qt.DropAction.CopyAction)
                event.accept()
                return True
            return False
        if t == QtCore.QEvent.Type.Drop:
            if self._handle_drop(event):
                return True
            return False
        return super().eventFilter(obj, event)

    def _accept_drag(self, event: QtGui.QDragEnterEvent | QtGui.QDragMoveEvent) -> bool:
        if not event.mimeData().hasUrls():
            return False
        for url in event.mimeData().urls():
            if url.isLocalFile():
                raw = url.toLocalFile()
                if raw:
                    try:
                        p = Path(raw)
                        if p.suffix.lower() in AUDIO_EXTENSIONS:
                            return True
                    except (OSError, ValueError):
                        pass
        return False

    def _handle_drop(self, event: QtGui.QDropEvent) -> bool:
        paths = []
        for url in event.mimeData().urls():
            if url.isLocalFile():
                raw = url.toLocalFile()
                if raw:
                    path = Path(raw)
                    if path.exists():
                        path = path.resolve()
                    if path.is_file() and is_audio(path):
                        paths.append(path)
        if paths:
            event.setDropAction(QtCore.Qt.DropAction.CopyAction)
            event.accept()
            self.files_dropped.emit(paths)
            return True
        return False

    def dragEnterEvent(self, event: QtGui.QDragEnterEvent):
        if self._accept_drag(event):
            event.setDropAction(QtCore.Qt.DropAction.CopyAction)
            event.accept()
        else:
            event.ignore()

    def dragMoveEvent(self, event: QtGui.QDragMoveEvent):
        if self._accept_drag(event):
            event.setDropAction(QtCore.Qt.DropAction.CopyAction)
            event.accept()
        else:
            event.ignore()

    def dropEvent(self, event: QtGui.QDropEvent):
        self._handle_drop(event)


class CircularProgressWidget(QtWidgets.QWidget):
    """Indicateur de progression circulaire (spinner ou pourcentage)."""
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setFixedSize(28, 28)
        self._value = -2
        self._angle = 0
        self._timer = QtCore.QTimer(self)
        self._timer.timeout.connect(self._tick)

    def set_progress(self, value: int):
        """-2 = vide, -1 = spinner, 0-100 = progression."""
        self._value = value
        if value == -1:
            if not self._timer.isActive():
                self._timer.start(50)
        else:
            self._timer.stop()
        self.update()

    def _tick(self):
        self._angle = (self._angle + 20) % 360
        self.update()

    def paintEvent(self, event):
        if self._value == -2:
            return
        p = QtGui.QPainter(self)
        p.setRenderHint(QtGui.QPainter.RenderHint.Antialiasing)
        r = min(self.width(), self.height()) / 2 - 3
        cx, cy = self.width() / 2, self.height() / 2
        if self._value == -1:
            pen = QtGui.QPen(QtGui.QColor(100, 100, 100), 3)
            pen.setCapStyle(QtCore.Qt.PenCapStyle.RoundCap)
            p.setPen(pen)
            p.setBrush(QtCore.Qt.BrushStyle.NoBrush)
            start = self._angle
            span = 90
            p.drawArc(int(cx - r), int(cy - r), int(r * 2), int(r * 2), start * 16, -span * 16)
        elif 0 <= self._value <= 100:
            pen = QtGui.QPen(QtGui.QColor(70, 130, 180), 2)
            p.setPen(pen)
            p.setBrush(QtCore.Qt.BrushStyle.NoBrush)
            span = int(360 * self._value / 100)
            p.drawArc(int(cx - r), int(cy - r), int(r * 2), int(r * 2), 90 * 16, -span * 16)


class AudioPlayer(QtWidgets.QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self._str_no_file = i18n.tr("player.no_file")
        self._str_play = i18n.tr("player.play")
        self._str_pause = i18n.tr("player.pause")
        self._str_stop = i18n.tr("player.stop")
        self._str_preview = i18n.tr("player.preview")
        self._str_return_full = i18n.tr("player.return_full")

        self.media_player = QtMultimedia.QMediaPlayer(self)
        self.audio_output = QtMultimedia.QAudioOutput(self)
        self.media_player.setAudioOutput(self.audio_output)

        layout = QtWidgets.QVBoxLayout(self)

        self.info_label = QtWidgets.QLabel(self._str_no_file)
        self.info_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.info_label)

        controls_layout = QtWidgets.QHBoxLayout()
        self.play_button = QtWidgets.QPushButton(self._str_play)
        self.stop_button = QtWidgets.QPushButton(self._str_stop)
        self.position_slider = QtWidgets.QSlider(QtCore.Qt.Orientation.Horizontal)
        self.position_slider.setEnabled(False)

        controls_layout.addWidget(self.play_button)
        controls_layout.addWidget(self.stop_button)
        layout.addLayout(controls_layout)
        layout.addWidget(self.position_slider)

        self.preview_button = QtWidgets.QPushButton(self._str_preview)
        self.preview_button.setEnabled(False)
        layout.addWidget(self.preview_button)

        self.return_button = QtWidgets.QPushButton(self._str_return_full)
        self.return_button.setEnabled(False)
        layout.addWidget(self.return_button)

        self.time_label = QtWidgets.QLabel("00:00 / 00:00")
        self.time_label.setAlignment(QtCore.Qt.AlignmentFlag.AlignCenter)
        layout.addWidget(self.time_label)

        self._gain_db = 0
        self._duration_ms = 0

        self.play_button.clicked.connect(self.toggle_playback)
        self.stop_button.clicked.connect(self.stop_playback)
        self.media_player.positionChanged.connect(self._update_slider_position)
        self.media_player.durationChanged.connect(self._update_duration_and_slider)
        self.position_slider.sliderMoved.connect(self.media_player.setPosition)

    def set_media(self, path: Path):
        media_url = QtCore.QUrl.fromLocalFile(str(path))
        self.media_player.setSource(media_url)
        self.info_label.setText(path.name)
        self.position_slider.setEnabled(True)
        self._update_time_label(self.media_player.position())

    def set_gain_db(self, gain_db: int):
        self._gain_db = gain_db
        linear = 10 ** (gain_db / 20)
        linear = max(0.0, min(linear, 1.0))
        self.audio_output.setVolume(linear)

    def toggle_playback(self):
        if self.media_player.source().isEmpty():
            return

        if self.media_player.playbackState() == QtMultimedia.QMediaPlayer.PlaybackState.PlayingState:
            self.media_player.pause()
            self.play_button.setText(self._str_play)
        else:
            self.media_player.play()
            self.play_button.setText(self._str_pause)

    def stop_playback(self):
        self.media_player.stop()
        self.play_button.setText(self._str_play)

    def set_ui_strings(
        self,
        no_file: str,
        play: str,
        pause: str,
        stop: str,
        preview: str,
        return_full: str,
    ):
        self._str_no_file = no_file
        self._str_play = play
        self._str_pause = pause
        self._str_stop = stop
        self._str_preview = preview
        self._str_return_full = return_full
        self.play_button.setText(play)
        self.stop_button.setText(stop)
        self.preview_button.setText(preview)
        self.return_button.setText(return_full)
        if self.media_player.source().isEmpty():
            self.info_label.setText(no_file)
        if self.media_player.playbackState() != QtMultimedia.QMediaPlayer.PlaybackState.PlayingState:
            self.play_button.setText(play)
        else:
            self.play_button.setText(pause)

    def get_no_file_text(self) -> str:
        return self._str_no_file

    def get_play_text(self) -> str:
        return self._str_play

    def get_pause_text(self) -> str:
        return self._str_pause

    def _update_slider_position(self, position: int):
        self.position_slider.blockSignals(True)
        self.position_slider.setValue(position)
        self.position_slider.blockSignals(False)
        self._update_time_label(position)

    def _update_duration_and_slider(self, duration: int):
        self._duration_ms = duration
        self.position_slider.setRange(0, duration)
        if duration == 0:
            self.position_slider.setEnabled(False)
        else:
            self.position_slider.setEnabled(True)
        self._update_time_label(self.media_player.position())

    def duration_seconds(self) -> float | None:
        return self._duration_ms / 1000 if self._duration_ms > 0 else None

    def _update_time_label(self, position_ms: int):
        current = self._format_time(position_ms)
        total = self._format_time(self._duration_ms)
        self.time_label.setText(f"{current} / {total}")

    @staticmethod
    def _format_time(milliseconds: int) -> str:
        if milliseconds <= 0:
            return "00:00"
        seconds = int(milliseconds / 1000)
        mins, secs = divmod(seconds, 60)
        hours, mins = divmod(mins, 60)
        if hours > 0:
            return f"{hours:02d}:{mins:02d}:{secs:02d}"
        return f"{mins:02d}:{secs:02d}"
