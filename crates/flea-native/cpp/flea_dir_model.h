#pragma once

#include <cstddef>
#include <cstdint>
#include <QAbstractListModel>
#include <QString>
#include <QVariantMap>
#include <QtQml/qqmlregistration.h>

extern "C" {
    struct FleaEntryC {
        const char* name;
        bool is_dir;
        bool is_link;
        uint64_t size;
        int64_t mtime;
        uint32_t mode;
        const char* icon;
        int kind;
        const char* target;
    };

    struct FleaDirectory;

    FleaDirectory* flea_native_scan(const char* path, bool hidden, int sort_by, bool sort_desc);
    void flea_native_free(FleaDirectory* dir);
    size_t flea_native_count(const FleaDirectory* dir);
    bool flea_native_get_entry(const FleaDirectory* dir, size_t index, FleaEntryC* out);
    FleaDirectory* flea_native_filter(const FleaDirectory* dir, const char* query);
}

class FleaDirModel : public QAbstractListModel {
    Q_OBJECT
    QML_ELEMENT

    Q_PROPERTY(QString path READ path WRITE setPath NOTIFY pathChanged)
    Q_PROPERTY(int count READ count NOTIFY countChanged)
    Q_PROPERTY(bool showHidden READ showHidden WRITE setShowHidden NOTIFY showHiddenChanged)
    Q_PROPERTY(int sortBy READ sortBy WRITE setSortBy NOTIFY sortByChanged)
    Q_PROPERTY(bool sortDesc READ sortDesc WRITE setSortDesc NOTIFY sortDescChanged)

public:
    enum Roles {
        NameRole = Qt::UserRole + 1,
        IsDirRole,
        IsLinkRole,
        SizeRole,
        MtimeRole,
        ModeRole,
        IconRole,
        ThumbRole,
        KindRole,
        TargetRole
    };

    explicit FleaDirModel(QObject* parent = nullptr);
    ~FleaDirModel() override;

    int rowCount(const QModelIndex& parent = QModelIndex()) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QHash<int, QByteArray> roleNames() const override;

    QString path() const { return m_path; }
    void setPath(const QString& path);

    int count() const;

    bool showHidden() const { return m_showHidden; }
    void setShowHidden(bool show);

    int sortBy() const { return m_sortBy; }
    void setSortBy(int sort);

    bool sortDesc() const { return m_sortDesc; }
    void setSortDesc(bool desc);

    Q_INVOKABLE QVariantMap get(int index) const;
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void search(const QString& query);

signals:
    void pathChanged();
    void countChanged();
    void showHiddenChanged();
    void sortByChanged();
    void sortDescChanged();

private:
    void rescan();

    QString m_path;
    bool m_showHidden = false;
    int m_sortBy = 0; // 0: name, 1: size, 2: date
    bool m_sortDesc = false;
    QString m_query;

    FleaDirectory* m_rawDir = nullptr;
    FleaDirectory* m_activeDir = nullptr;
};
