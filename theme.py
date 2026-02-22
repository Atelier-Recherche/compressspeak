# -*- coding: utf-8 -*-
"""Gestion du thème (clair/sombre) pour Compress Speak."""

from PyQt6 import QtCore, QtGui, QtWidgets


class ThemeManager:
    def __init__(self, app: QtWidgets.QApplication):
        self.app = app
        self.current_theme = "dark"
        self.apply_dark()

    def toggle(self):
        if self.current_theme == "dark":
            self.apply_light()
        else:
            self.apply_dark()

    def apply_dark(self):
        palette = QtGui.QPalette()
        palette.setColor(QtGui.QPalette.ColorRole.Window, QtGui.QColor(36, 36, 36))
        palette.setColor(QtGui.QPalette.ColorRole.WindowText, QtCore.Qt.GlobalColor.white)
        palette.setColor(QtGui.QPalette.ColorRole.Base, QtGui.QColor(25, 25, 25))
        palette.setColor(QtGui.QPalette.ColorRole.AlternateBase, QtGui.QColor(45, 45, 45))
        palette.setColor(QtGui.QPalette.ColorRole.ToolTipBase, QtCore.Qt.GlobalColor.white)
        palette.setColor(QtGui.QPalette.ColorRole.ToolTipText, QtCore.Qt.GlobalColor.white)
        palette.setColor(QtGui.QPalette.ColorRole.Text, QtCore.Qt.GlobalColor.white)
        palette.setColor(QtGui.QPalette.ColorRole.Button, QtGui.QColor(45, 45, 45))
        palette.setColor(QtGui.QPalette.ColorRole.ButtonText, QtCore.Qt.GlobalColor.white)
        palette.setColor(QtGui.QPalette.ColorRole.BrightText, QtCore.Qt.GlobalColor.red)
        palette.setColor(QtGui.QPalette.ColorRole.Highlight, QtGui.QColor(0, 120, 215))
        palette.setColor(QtGui.QPalette.ColorRole.HighlightedText, QtCore.Qt.GlobalColor.black)
        self.app.setPalette(palette)
        self.app.setStyleSheet("""
            QToolTip {
                background-color: #2d2d2d;
                color: #ffffff;
                border: 1px solid #555555;
                padding: 4px;
            }
        """)
        self.current_theme = "dark"

    def apply_light(self):
        palette = QtGui.QPalette()
        palette.setColor(QtGui.QPalette.ColorRole.Window, QtGui.QColor(250, 250, 250))
        palette.setColor(QtGui.QPalette.ColorRole.WindowText, QtGui.QColor(25, 25, 25))
        palette.setColor(QtGui.QPalette.ColorRole.Base, QtGui.QColor(255, 255, 255))
        palette.setColor(QtGui.QPalette.ColorRole.AlternateBase, QtGui.QColor(243, 243, 243))
        palette.setColor(QtGui.QPalette.ColorRole.ToolTipBase, QtGui.QColor(255, 255, 220))
        palette.setColor(QtGui.QPalette.ColorRole.ToolTipText, QtGui.QColor(25, 25, 25))
        palette.setColor(QtGui.QPalette.ColorRole.Text, QtGui.QColor(25, 25, 25))
        palette.setColor(QtGui.QPalette.ColorRole.PlaceholderText, QtGui.QColor(120, 120, 120))
        palette.setColor(QtGui.QPalette.ColorRole.Button, QtGui.QColor(240, 240, 240))
        palette.setColor(QtGui.QPalette.ColorRole.ButtonText, QtGui.QColor(25, 25, 25))
        palette.setColor(QtGui.QPalette.ColorRole.BrightText, QtGui.QColor(180, 0, 0))
        palette.setColor(QtGui.QPalette.ColorRole.Highlight, QtGui.QColor(0, 120, 215))
        palette.setColor(QtGui.QPalette.ColorRole.HighlightedText, QtGui.QColor(255, 255, 255))
        palette.setColor(QtGui.QPalette.ColorGroup.Disabled, QtGui.QPalette.ColorRole.WindowText, QtGui.QColor(120, 120, 120))
        palette.setColor(QtGui.QPalette.ColorGroup.Disabled, QtGui.QPalette.ColorRole.Text, QtGui.QColor(120, 120, 120))
        palette.setColor(QtGui.QPalette.ColorGroup.Disabled, QtGui.QPalette.ColorRole.ButtonText, QtGui.QColor(120, 120, 120))
        self.app.setPalette(palette)
        self.app.setStyleSheet("""
            QToolTip {
                background-color: #FFFDE7;
                color: #191919;
                border: 1px solid #cccccc;
                padding: 4px;
            }
        """)
        self.current_theme = "light"
