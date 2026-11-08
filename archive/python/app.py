# -*- coding: utf-8 -*-
"""Point d'entrée de Compress Speak."""

import sys
import os

# IMPORTANT : Importer ONNX Runtime ET audio_separator AVANT PyQt6 pour éviter les conflits de DLL avec DirectML
if sys.platform == "win32":
    import site
    for sp in site.getsitepackages():
        if os.path.isdir(sp):
            try:
                os.add_dll_directory(sp)
            except OSError:
                pass
        capi = os.path.join(sp, "onnxruntime", "capi")
        if os.path.isdir(capi):
            try:
                os.add_dll_directory(capi)
            except OSError:
                pass

# Désactiver tqdm AVANT tout import pour éviter les barres de progression
os.environ["TQDM_DISABLE"] = "1"

_directml_available = False
try:
    import onnxruntime
    providers = onnxruntime.get_available_providers()
    _directml_available = 'DmlExecutionProvider' in providers
    print(f"ONNX Runtime chargé. Providers: {providers}")
    
    # Importer audio_separator aussi pour éviter les problèmes de DLL dans les threads
    from audio_separator.separator import Separator
    print("audio_separator chargé avec succès")
    
    if _directml_available:
        print(">>> DirectML DISPONIBLE pour accélération GPU AMD/Intel <<<")
    else:
        print("DirectML non disponible, traitement en CPU")
        
except Exception as e:
    print(f"ATTENTION: Impossible de charger ONNX Runtime / audio_separator: {e}")
    _directml_available = False

from PyQt6 import QtCore, QtWidgets

import i18n
from main_window import MainWindow
from theme import ThemeManager


def main():
    app = QtWidgets.QApplication(sys.argv)
    app.setApplicationName("Compress Speak")
    app.setOrganizationName("CompressSpeak")
    settings = QtCore.QSettings()
    settings.beginGroup(i18n.SETTINGS_GROUP)
    lang = settings.value(i18n.SETTINGS_LANG, "fr")
    settings.endGroup()
    if lang in ("fr", "en", "de", "es"):
        i18n.set_language(lang)
    theme_manager = ThemeManager(app)
    window = MainWindow(theme_manager)
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
