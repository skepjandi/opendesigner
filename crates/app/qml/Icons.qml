pragma Singleton
import QtQuick

// The vendored Lucide icons (qml/icons/*.svg, ISC; see qml/icons/LICENSE), as `IconSet` holds
// them. `source(name, color)` returns the drawing as a data URL with the stroke set to `color`, so an
// icon follows the theme and the hover and active states without extra Qt modules.
QtObject {
    function raw(name) { return IconSet.svgs[name] || "" }

    function hex(c) {
        return "#" + [c.r, c.g, c.b].map(v => Math.round(v * 255).toString(16).padStart(2, "0")).join("")
    }

    function source(name, color) {
        const svg = raw(name)
        if (!svg) return ""
        return "data:image/svg+xml;utf8," + encodeURIComponent(svg.replace(/currentColor/g, hex(color)))
    }
}
