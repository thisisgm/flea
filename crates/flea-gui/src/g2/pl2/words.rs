//! Translated labels cached when the language changes, outside paint.
use flea::i18n::I18n;
use flea::ui_logic_pl2::form::PROTOCOLS;

pub struct Words {
    pub tracking: f32,
    pub title: String,
    pub retry_title: String,
    pub failed_title: String,
    pub escape: String,
    pub protocols: [String; 5],
    pub fields: [String; 7],
    pub path: String,
    pub export: String,
    pub host_hint: String,
    pub password_hint: String,
    pub nfs_hint: String,
    pub mounts_as: String,
    pub cancel: String,
    pub connect: String,
    pub retry: String,
    pub connecting: String,
    pub saving: String,
    pub close: String,
    pub tls: String,
    pub dropbox: String,
    pub install: String,
}

impl Words {
    pub fn new(catalog: &I18n) -> Self {
        let tr = |key| catalog.tr(key);
        let heading = |key| flea::ui_logic_pl2::heading::heading(&tr(key), catalog.language()).0;
        Self {
            tracking: if flea::ui_logic_pl2::heading::heading(
                &tr("network.host"),
                catalog.language(),
            )
            .1
            {
                1.2
            } else {
                0.
            },
            title: tr("network.title"),
            retry_title: tr("network.retry_title"),
            failed_title: tr("network.failed_title"),
            escape: tr("network.escape"),
            protocols: PROTOCOLS.map(|p| tr(p.key())),
            fields: [
                "network.label",
                "network.host",
                "network.port",
                "network.share",
                "network.domain",
                "network.username",
                "network.password",
            ]
            .map(heading),
            path: heading("network.path"),
            export: heading("network.export"),
            host_hint: tr("network.host_hint"),
            password_hint: tr("network.password_hint"),
            nfs_hint: tr("network.nfs_hint"),
            mounts_as: tr("network.mounts_as"),
            cancel: tr("network.cancel"),
            connect: tr("network.connect"),
            retry: tr("network.retry"),
            connecting: tr("network.connecting"),
            saving: tr("network.saving"),
            close: tr("network.close"),
            tls: tr("network.tls"),
            dropbox: heading("network.dropbox"),
            install: tr("network.install_dropbox"),
        }
    }
}
