#include "flea_dir_model.h"

FleaDirModel::FleaDirModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

FleaDirModel::~FleaDirModel() {
    if (m_activeDir && m_activeDir != m_rawDir) {
        flea_native_free(m_activeDir);
    }
    if (m_rawDir) {
        flea_native_free(m_rawDir);
    }
}

int FleaDirModel::rowCount(const QModelIndex& parent) const {
    if (parent.isValid() || !m_activeDir) {
        return 0;
    }
    return static_cast<int>(flea_native_count(m_activeDir));
}

int FleaDirModel::count() const {
    return rowCount();
}

QVariant FleaDirModel::data(const QModelIndex& index, int role) const {
    if (!index.isValid() || !m_activeDir) {
        return QVariant();
    }

    int row = index.row();
    FleaEntryC e;
    if (!flea_native_get_entry(m_activeDir, static_cast<size_t>(row), &e)) {
        return QVariant();
    }

    switch (role) {
    case NameRole:
    case Qt::DisplayRole:
        return QString::fromUtf8(e.name ? e.name : "");
    case IsDirRole:
        return e.is_dir;
    case IsLinkRole:
        return e.is_link;
    case SizeRole:
        return static_cast<qulonglong>(e.size);
    case MtimeRole:
        return static_cast<qlonglong>(e.mtime);
    case ModeRole:
        return e.mode;
    case IconRole:
        return QString::fromUtf8(e.icon ? e.icon : "");
    case ThumbRole:
        return false;
    case KindRole:
        return e.kind;
    case TargetRole:
        return QString::fromUtf8(e.target ? e.target : "");
    default:
        return QVariant();
    }
}

QHash<int, QByteArray> FleaDirModel::roleNames() const {
    QHash<int, QByteArray> roles;
    roles[NameRole] = "n";
    roles[IsDirRole] = "d";
    roles[IsLinkRole] = "l";
    roles[SizeRole] = "s";
    roles[MtimeRole] = "m";
    roles[ModeRole] = "p";
    roles[IconRole] = "i";
    roles[ThumbRole] = "t";
    roles[KindRole] = "k";
    roles[TargetRole] = "target";
    return roles;
}

QVariantMap FleaDirModel::get(int index) const {
    QVariantMap map;
    if (!m_activeDir || index < 0) {
        return map;
    }

    FleaEntryC e;
    if (!flea_native_get_entry(m_activeDir, static_cast<size_t>(index), &e)) {
        return map;
    }

    map["n"] = QString::fromUtf8(e.name ? e.name : "");
    map["d"] = e.is_dir;
    if (e.is_link) {
        map["l"] = QString::fromUtf8(e.target ? e.target : "");
    }
    map["s"] = static_cast<qulonglong>(e.size);
    map["m"] = static_cast<qlonglong>(e.mtime);
    map["p"] = e.mode;
    map["i"] = QString::fromUtf8(e.icon ? e.icon : "");
    map["t"] = false;
    map["k"] = e.kind;
    return map;
}

void FleaDirModel::setPath(const QString& path) {
    if (m_path == path) return;
    m_path = path;
    emit pathChanged();
    rescan();
}

void FleaDirModel::setShowHidden(bool show) {
    if (m_showHidden == show) return;
    m_showHidden = show;
    emit showHiddenChanged();
    rescan();
}

void FleaDirModel::setSortBy(int sort) {
    if (m_sortBy == sort) return;
    m_sortBy = sort;
    emit sortByChanged();
    rescan();
}

void FleaDirModel::setSortDesc(bool desc) {
    if (m_sortDesc == desc) return;
    m_sortDesc = desc;
    emit sortDescChanged();
    rescan();
}

void FleaDirModel::refresh() {
    rescan();
}

void FleaDirModel::search(const QString& query) {
    m_query = query;
    if (!m_rawDir) return;

    beginResetModel();
    if (m_activeDir && m_activeDir != m_rawDir) {
        flea_native_free(m_activeDir);
    }

    QByteArray qba = query.toUtf8();
    if (query.isEmpty()) {
        m_activeDir = m_rawDir;
    } else {
        m_activeDir = flea_native_filter(m_rawDir, qba.constData());
    }
    endResetModel();
    emit countChanged();
}

void FleaDirModel::rescan() {
    if (m_path.isEmpty()) {
        beginResetModel();
        if (m_activeDir && m_activeDir != m_rawDir) {
            flea_native_free(m_activeDir);
        }
        if (m_rawDir) {
            flea_native_free(m_rawDir);
        }
        m_rawDir = nullptr;
        m_activeDir = nullptr;
        endResetModel();
        emit countChanged();
        return;
    }

    QByteArray pba = m_path.toUtf8();
    FleaDirectory* newDir = flea_native_scan(pba.constData(), m_showHidden, m_sortBy, m_sortDesc);

    beginResetModel();
    if (m_activeDir && m_activeDir != m_rawDir) {
        flea_native_free(m_activeDir);
    }
    if (m_rawDir) {
        flea_native_free(m_rawDir);
    }
    m_rawDir = newDir;
    m_activeDir = newDir;
    if (m_rawDir && !m_query.isEmpty()) {
        QByteArray qba = m_query.toUtf8();
        FleaDirectory* filtered = flea_native_filter(m_rawDir, qba.constData());
        if (filtered) m_activeDir = filtered;
    }
    endResetModel();
    emit countChanged();
}
