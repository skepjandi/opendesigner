#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(QString, greeting)]
        type Model = super::ModelRust;
    }
}

use cxx_qt_lib::QString;

pub struct ModelRust {
    greeting: QString,
}

impl Default for ModelRust {
    fn default() -> Self {
        ModelRust {
            greeting: QString::from("hello from rust"),
        }
    }
}
