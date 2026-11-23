import QtQuick
import QtWebEngine

// The one place QtWebEngine is imported, so the rest of the editor still loads where it is
// missing (WebPanel falls back to the system browser). It only ever shows the local address of a
// service in the approved run plan; any other address is opened in the system browser instead.
Item {
    id: root
    property url address
    property int zoomPercent: 100
    function reload() { view.reload() }

    WebEngineProfile { id: profile; offTheRecord: true }

    WebEngineView {
        id: view
        anchors.fill: parent
        profile: profile
        url: root.address
        zoomFactor: root.zoomPercent / 100
        settings.javascriptCanOpenWindows: false
        settings.localContentCanAccessRemoteUrls: false
        onNavigationRequested: request => {
            const host = request.url.toString().replace(/^[a-z]+:\/\//, "").split(/[:/]/)[0]
            if (host !== "localhost" && host !== "127.0.0.1" && host !== "[::1]") {
                request.action = WebEngineNavigationRequest.IgnoreRequest
                Qt.openUrlExternally(request.url)
            }
        }
    }
}
