// The one C++ file of the editor: an image provider that hands pictures drawn by the Rust core to
// Qt Quick as raw pixels (the canvas, the video preview, timeline thumbnails). Encoding them as
// PNGs only for Qt to decode again cost more than drawing them (docs/wiki/qt-desktop.md#canvas).
// A picture is published under an id and shown with `image://pictures/<id>`.

#include <QHash>
#include <QImage>
#include <QMutex>
#include <QMutexLocker>
#include <QQmlApplicationEngine>
#include <QQuickImageProvider>
#include <QString>

#include <cstddef>
#include <cstring>
#include <utility>

namespace {

QMutex picturesLock;
QHash<QString, QImage> pictures;

class PictureProvider : public QQuickImageProvider {
public:
    PictureProvider() : QQuickImageProvider(QQuickImageProvider::Image) {}

    QImage requestImage(const QString &id, QSize *size, const QSize &) override {
        QMutexLocker lock(&picturesLock);
        const QImage image = pictures.value(id);
        if (size != nullptr) {
            *size = image.size();
        }
        return image;
    }
};

} // namespace

extern "C" {

// Copies `width` x `height` premultiplied RGBA pixels (4 bytes each, rows packed) and serves them
// as `id` (a NUL-terminated UTF-8 string). Returns false when Qt cannot hold an image that size.
bool od_picture_publish(const char *id, const unsigned char *pixels, int width, int height) {
    QImage image(width, height, QImage::Format_RGBA8888_Premultiplied);
    if (image.isNull()) {
        return false;
    }
    std::memcpy(image.bits(), pixels, static_cast<std::size_t>(width) * static_cast<std::size_t>(height) * 4);
    QMutexLocker lock(&picturesLock);
    pictures.insert(QString::fromUtf8(id), std::move(image));
    return true;
}

void od_picture_forget(const char *id) {
    QMutexLocker lock(&picturesLock);
    pictures.remove(QString::fromUtf8(id));
}

void od_picture_forget_prefix(const char *prefix) {
    const QString start = QString::fromUtf8(prefix);
    QMutexLocker lock(&picturesLock);
    for (auto it = pictures.begin(); it != pictures.end();) {
        it = it.key().startsWith(start) ? pictures.erase(it) : std::next(it);
    }
}

// Registers the provider as `image://pictures/`. The engine owns it from here.
void od_pictures_install(QQmlApplicationEngine *engine) {
    engine->addImageProvider(QStringLiteral("pictures"), new PictureProvider);
}

}
