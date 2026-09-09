.import "../../ui/js/PickerA11y.js" as A11y

function run(check) {
    check("rename names the target", A11y.renameLabel("photo.jpg"), "Rename photo.jpg")
    check("rename has a fallback", A11y.renameLabel(""), "Rename item")

    var notice = A11y.notice("Copied 3 items", false)
    check("completion is announced", notice.text, "Copied 3 items")
    check("completion is polite", notice.assertive, false)
    notice = A11y.notice("Moved 0 items, 2 failed", true)
    check("an error keeps its text", notice.text, "Moved 0 items, 2 failed")
    check("an error is assertive", notice.assertive, true)
    check("an empty notice stays silent", A11y.notice("", true).text, "")

    var progress = A11y.progress(A11y.emptyProgress(), "transfer:7", "Copying 1 of 4", 0)
    check("transfer start is announced", progress.text, "Copying 1 of 4")
    check("start records its operation", progress.key, "transfer:7")
    progress = A11y.progress(progress, "transfer:7", "Copying 1 of 4, a.txt", 0.24)
    check("a sample below 25 percent stays silent", progress.text, "")
    progress = A11y.progress(progress, "transfer:7", "Copying 2 of 4, b.txt", 0.25)
    check("25 percent is announced", progress.text, "Copying 2 of 4, b.txt, 25% complete")
    progress = A11y.progress(progress, "transfer:7", "Copying 2 of 4, b.txt", 0.49)
    check("repeated first-quarter progress stays silent", progress.text, "")
    progress = A11y.progress(progress, "transfer:7", "Copying 4 of 4, d.txt", 0.76)
    check("a jump announces the newest boundary once", progress.text,
          "Copying 4 of 4, d.txt, 75% complete")
    progress = A11y.progress(progress, "transfer:7", "Copying 4 of 4, d.txt", 1)
    check("100 percent waits for the completion notice", progress.text, "")

    progress = A11y.progress(progress, "", "", 0)
    check("ending resets progress", progress.key + ":" + progress.step, ":0")
    progress = A11y.progress(progress, "fetch", "Fetching report.pdf", 0)
    check("an indeterminate fetch is announced once", progress.text, "Fetching report.pdf")
    progress = A11y.progress(progress, "fetch", "Fetching report.pdf · 1.2 MB", 0)
    check("unknown-size fetch samples stay silent", progress.text, "")
}
