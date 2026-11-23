// Qt WebEngine (Chromium, offered under the LGPLv3) shows a running web service inside the
// editor. Qt requires its initializer to run before the QGuiApplication exists, so the program
// calls this first (docs/wiki/code-mode.md#web-view).

#include <QtWebEngineQuick/qtwebenginequickglobal.h>

extern "C" void od_init_web_engine() {
    QtWebEngineQuick::initialize();
}
