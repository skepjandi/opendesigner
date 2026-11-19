import QtQuick

// A list model fed with arrays of plain objects that change often (layers, chat lines, outlines of
// what an agent touched). `sync(items)` changes only what differs: an item that is still there
// keeps its delegate, so nothing is rebuilt, scroll positions and focus stay, and nothing flashes.
// Each row holds `key` and `json`; a delegate reads the object as `JSON.parse(json)`.
//
// An item is identified by its `id` field, or by its position when it has none.
ListModel {
    id: model

    function keyOf(item, i) {
        const k = item ? item.id : undefined
        return k === undefined || k === null ? "#" + i : String(k)
    }

    function sync(items) {
        const list = items || []
        const keys = []
        const jsons = []
        const wanted = {}
        for (let i = 0; i < list.length; ++i) {
            const key = keyOf(list[i], i)
            keys.push(key)
            jsons.push(JSON.stringify(list[i]))
            wanted[key] = true
        }
        for (let i = model.count - 1; i >= 0; --i)
            if (!wanted[model.get(i).key]) model.remove(i)
        for (let i = 0; i < keys.length; ++i) {
            if (i < model.count && model.get(i).key === keys[i]) {
                if (model.get(i).json !== jsons[i]) model.setProperty(i, "json", jsons[i])
                continue
            }
            let from = -1
            for (let j = i + 1; j < model.count; ++j) {
                if (model.get(j).key === keys[i]) { from = j; break }
            }
            if (from >= 0) {
                model.move(from, i, 1)
                if (model.get(i).json !== jsons[i]) model.setProperty(i, "json", jsons[i])
            } else {
                model.insert(i, { key: keys[i], json: jsons[i] })
            }
        }
        while (model.count > keys.length) model.remove(model.count - 1)
    }
}
