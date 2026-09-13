.pragma library

// Generated from keys.toml by tools/flea-keymap-gen. Do not edit.
var PRESETS = ["default","vim","mac","windows"]
var preset = "default"
var PRESET_KEYS = [
    {"mods":"ctrl","key":"1","keys":"ctrl-1","action":"viewList","label":"list view","context":"listing","frontend":"all","preset":"default","code":"Key_1","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_1,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"2","keys":"ctrl-2","action":"viewColumns","label":"columns view","context":"listing","frontend":"all","preset":"default","code":"Key_2","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_2,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"3","keys":"ctrl-3","action":"viewGrid","label":"grid view","context":"listing","frontend":"all","preset":"default","code":"Key_3","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_3,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"1","keys":"ctrl-1","action":"viewList","label":"list view","context":"listing","frontend":"all","preset":"vim","code":"Key_1","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_1,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"2","keys":"ctrl-2","action":"viewColumns","label":"columns view","context":"listing","frontend":"all","preset":"vim","code":"Key_2","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_2,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"3","keys":"ctrl-3","action":"viewGrid","label":"grid view","context":"listing","frontend":"all","preset":"vim","code":"Key_3","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_3,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"1","keys":"ctrl-1","action":"viewList","label":"list view","context":"listing","frontend":"all","preset":"mac","code":"Key_1","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_1,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"2","keys":"ctrl-2","action":"viewColumns","label":"columns view","context":"listing","frontend":"all","preset":"mac","code":"Key_2","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_2,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"3","keys":"ctrl-3","action":"viewGrid","label":"grid view","context":"listing","frontend":"all","preset":"mac","code":"Key_3","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_3,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Up","keys":"ctrl-up","action":"parent","label":"up one level","context":"listing","frontend":"all","preset":"mac","code":"Key_Up","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Down","keys":"ctrl-down","action":"open","label":"open","context":"listing","frontend":"all","preset":"mac","code":"Key_Down","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Delete","keys":"ctrl-delete","action":"trash","label":"move to trash","context":"listing","frontend":"all","preset":"mac","code":"Key_Delete","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Delete,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"K","keys":"ctrl-k","action":"addNetwork","label":"connect to server","context":"listing","frontend":"all","preset":"mac","code":"Key_K","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_K,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"H","keys":"ctrl-h","action":"toggleHidden","label":"hidden files","context":"listing","frontend":"all","preset":"windows","code":"Key_H","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_H,"mask":(Qt.ControlModifier)},
    {"mods":"ctrlshift","key":"1","keys":"ctrl-shift-1","action":"viewList","label":"list view","context":"listing","frontend":"all","preset":"windows","code":"Key_1","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_1,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"2","keys":"ctrl-shift-2","action":"viewColumns","label":"columns view","context":"listing","frontend":"all","preset":"windows","code":"Key_2","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_2,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"3","keys":"ctrl-shift-3","action":"viewGrid","label":"grid view","context":"listing","frontend":"all","preset":"windows","code":"Key_3","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_3,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrl","key":"N","keys":"ctrl-n","action":"windowNew","label":"new window","frontend":"gui","context":"listing","preset":"default","code":"Key_N","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_N,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"N","keys":"ctrl-n","action":"windowNew","label":"new window","frontend":"gui","context":"listing","preset":"vim","code":"Key_N","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_N,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"N","keys":"ctrl-n","action":"windowNew","label":"new window","frontend":"gui","context":"listing","preset":"windows","code":"Key_N","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_N,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"T","keys":"ctrl-t","action":"tabNew","label":"new tab","frontend":"all","context":"listing","preset":"mac","code":"Key_T","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_T,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"T","keys":"ctrl-t","action":"tabNew","label":"new tab","frontend":"all","context":"listing","preset":"windows","code":"Key_T","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_T,"mask":(Qt.ControlModifier)},
    {"mods":"shift","key":"Delete","keys":"shift-delete","action":"deletePermanently","label":"delete permanently","frontend":"all","context":"listing","preset":"mac","code":"Key_Delete","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Delete,"mask":(Qt.ShiftModifier)},
    {"mods":"shift","key":"Delete","keys":"shift-delete","action":"deletePermanently","label":"delete permanently","frontend":"all","context":"listing","preset":"windows","code":"Key_Delete","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Delete,"mask":(Qt.ShiftModifier)},
    {"mods":"alt","key":"P","keys":"alt-p","action":"togglePreview","label":"toggle preview column","frontend":"all","context":"listing","preset":"all","code":"Key_P","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_P,"mask":(Qt.AltModifier)},
    {"mods":"ctrl","key":"Space","keys":"ctrl-space","action":"loadPreview","label":"load selected into column","frontend":"all","context":"listing","preset":"all","code":"Key_Space","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Space,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"W","keys":"ctrl-w","action":"tabClose","label":"close tab","frontend":"all","context":"listing","preset":"all","code":"Key_W","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_W,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"PageDown","keys":"ctrl-pagedown","action":"tabNext","label":"next tab","frontend":"all","context":"listing","preset":"all","code":"Key_PageDown","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_PageDown,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"PageUp","keys":"ctrl-pageup","action":"tabPrevious","label":"previous tab","frontend":"all","context":"listing","preset":"all","code":"Key_PageUp","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_PageUp,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Tab","keys":"ctrl-tab","action":"focusPreview","label":"focus preview / list","frontend":"all","context":"listing","preset":"all","code":"Key_Tab","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Insert","keys":"ctrl-insert","action":"copy","label":"copy","frontend":"tui","context":"listing","preset":"all","code":"Key_Insert","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Insert,"mask":(Qt.ControlModifier)},
    {"mods":"shift","key":"Insert","keys":"shift-insert","action":"paste","label":"paste","frontend":"tui","context":"listing","preset":"all","code":"Key_Insert","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Insert,"mask":(Qt.ShiftModifier)},
    {"mods":"text","key":"H","keys":"h","action":"historyBack","label":"history back","frontend":"all","context":"listing","preset":"default","code":"","text":"H","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"H","keys":"h","action":"historyBack","label":"history back","frontend":"all","context":"listing","preset":"vim","code":"","text":"H","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"L","keys":"l","action":"historyForward","label":"history forward","frontend":"all","context":"listing","preset":"default","code":"","text":"L","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"L","keys":"l","action":"historyForward","label":"history forward","frontend":"all","context":"listing","preset":"vim","code":"","text":"L","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"y","keys":"yy","action":"copyArm","label":"copy","frontend":"all","context":"listing","preset":"vim","code":"","text":"y","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"d","keys":"dd","action":"cutArm","label":"cut","frontend":"all","context":"listing","preset":"vim","code":"","text":"d","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"p","keys":"pp","action":"pasteArm","label":"paste","frontend":"all","context":"listing","preset":"vim","code":"","text":"p","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"g","keys":"gg","action":"cursorFirstArm","label":"first row","frontend":"all","context":"listing","preset":"vim","code":"","text":"g","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"D","keys":"D","action":"trash","label":"move to Trash","frontend":"all","context":"listing","preset":"vim","code":"","text":"D","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"u","keys":"u","action":"undo","label":"undo","frontend":"all","context":"listing","preset":"vim","code":"","text":"u","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"l","keys":"l","action":"open","label":"open","frontend":"all","context":"listing","preset":"vim","code":"","text":"l","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"none","key":"Return","keys":"enter","action":"rename","label":"rename","frontend":"all","context":"listing","preset":"mac","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Return,"mask":(0)},
    {"mods":"none","key":"Enter","keys":"enter","action":"rename","label":"rename","frontend":"all","context":"listing","preset":"mac","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Enter,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"parent","label":"up one level","frontend":"all","context":"listing","preset":"mac","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Right","keys":"right","action":"open","label":"open","frontend":"all","context":"listing","preset":"mac","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"ctrl","key":"X","keys":"ctrl-x","action":"","label":"Control+X is inert","frontend":"all","context":"listing","preset":"mac","code":"Key_X","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_X,"mask":(Qt.ControlModifier)},
    {"mods":"super","key":"C","keys":"super-c","action":"copy","label":"copy","frontend":"all","context":"listing","preset":"mac","code":"Key_C","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_C,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"V","keys":"super-v","action":"paste","label":"paste","frontend":"all","context":"listing","preset":"mac","code":"Key_V","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_V,"mask":(Qt.MetaModifier)},
    {"mods":"superalt","key":"V","keys":"superalt-v","action":"movePaste","label":"move copied items here","frontend":"all","context":"listing","preset":"mac","code":"Key_V","text":"","ctrl":false,"shift":false,"alt":true,"super":true,"keycode":Qt.Key_V,"mask":(Qt.MetaModifier | Qt.AltModifier)},
    {"mods":"super","key":"D","keys":"super-d","action":"duplicate","label":"duplicate","frontend":"all","context":"listing","preset":"mac","code":"Key_D","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_D,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"Z","keys":"super-z","action":"undo","label":"undo","frontend":"all","context":"listing","preset":"mac","code":"Key_Z","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_Z,"mask":(Qt.MetaModifier)},
    {"mods":"supershift","key":"Z","keys":"supershift-z","action":"redo","label":"redo","frontend":"all","context":"listing","preset":"mac","code":"Key_Z","text":"","ctrl":false,"shift":true,"alt":false,"super":true,"keycode":Qt.Key_Z,"mask":(Qt.MetaModifier | Qt.ShiftModifier)},
    {"mods":"super","key":"A","keys":"super-a","action":"selectAll","label":"select all","frontend":"all","context":"listing","preset":"mac","code":"Key_A","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_A,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"I","keys":"super-i","action":"properties","label":"get info","frontend":"all","context":"listing","preset":"mac","code":"Key_I","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_I,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"N","keys":"super-n","action":"windowNew","label":"new window","frontend":"gui","context":"listing","preset":"mac","code":"Key_N","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_N,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"BracketLeft","keys":"super-bracketleft","action":"historyBack","label":"history back","frontend":"all","context":"listing","preset":"mac","code":"Key_BracketLeft","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_BracketLeft,"mask":(Qt.MetaModifier)},
    {"mods":"super","key":"BracketRight","keys":"super-bracketright","action":"historyForward","label":"history forward","frontend":"all","context":"listing","preset":"mac","code":"Key_BracketRight","text":"","ctrl":false,"shift":false,"alt":false,"super":true,"keycode":Qt.Key_BracketRight,"mask":(Qt.MetaModifier)},
    {"mods":"supershift","key":"Period","keys":"supershift-period","action":"toggleHidden","label":"hidden files","frontend":"all","context":"listing","preset":"mac","code":"Key_Period","text":"","ctrl":false,"shift":true,"alt":false,"super":true,"keycode":Qt.Key_Period,"mask":(Qt.MetaModifier | Qt.ShiftModifier)},
    {"mods":"supershift","key":"Greater","keys":"supershift-greater","action":"toggleHidden","label":"hidden files","frontend":"all","context":"listing","preset":"mac","code":"Key_Greater","text":"","ctrl":false,"shift":true,"alt":false,"super":true,"keycode":Qt.Key_Greater,"mask":(Qt.MetaModifier | Qt.ShiftModifier)},
    {"mods":"ctrl","key":"D","keys":"ctrl-d","action":"trash","label":"move to Trash","frontend":"all","context":"listing","preset":"windows","code":"Key_D","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_D,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Y","keys":"ctrl-y","action":"redo","label":"redo","frontend":"all","context":"listing","preset":"windows","code":"Key_Y","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Y,"mask":(Qt.ControlModifier)},
    {"mods":"alt","key":"Return","keys":"alt-return","action":"properties","label":"properties","frontend":"all","context":"listing","preset":"windows","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_Return,"mask":(Qt.AltModifier)},
    {"mods":"alt","key":"Enter","keys":"alt-enter","action":"properties","label":"properties","frontend":"all","context":"listing","preset":"windows","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_Enter,"mask":(Qt.AltModifier)},
    {"mods":"alt","key":"Left","keys":"alt-left","action":"historyBack","label":"history back","frontend":"all","context":"listing","preset":"windows","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_Left,"mask":(Qt.AltModifier)},
    {"mods":"alt","key":"Right","keys":"alt-right","action":"historyForward","label":"history forward","frontend":"all","context":"listing","preset":"windows","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_Right,"mask":(Qt.AltModifier)},
    {"mods":"alt","key":"Up","keys":"alt-up","action":"parent","label":"up one level","frontend":"all","context":"listing","preset":"windows","code":"Key_Up","text":"","ctrl":false,"shift":false,"alt":true,"super":false,"keycode":Qt.Key_Up,"mask":(Qt.AltModifier)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","label":"escape","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
    {"mods":"none","key":"Menu","keys":"menu","action":"menu","label":"context menu","frontend":"all","context":"listing,rail","preset":"all","code":"Key_Menu","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Menu,"mask":(0)},
    {"mods":"shift","key":"F10","keys":"shift-f10","action":"menu","label":"context menu","frontend":"all","context":"listing,rail","preset":"all","code":"Key_F10","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_F10,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Down","keys":"down","action":"cursorDown","label":"cursorDown","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Down","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(0)},
    {"mods":"none","key":"Up","keys":"up","action":"cursorUp","label":"cursorUp","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Up","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(0)},
    {"mods":"text","key":"j","keys":"j","action":"cursorDown","label":"cursorDown","frontend":"all","context":"rail,menu,panel,preview,media","preset":"all","code":"","text":"j","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"p","keys":"p","action":"playPause","label":"playPause","frontend":"all","context":"media","preset":"all","code":"","text":"p","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"k","keys":"k","action":"cursorUp","label":"cursorUp","frontend":"all","context":"rail,menu,panel,preview,media","preset":"all","code":"","text":"k","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"none","key":"Return","keys":"return","action":"open","label":"open","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Return,"mask":(0)},
    {"mods":"none","key":"Enter","keys":"enter","action":"open","label":"open","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Enter,"mask":(0)},
    {"mods":"none","key":"Space","keys":"space","action":"preview","label":"preview","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Space","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Space,"mask":(0)},
    {"mods":"none","key":"Tab","keys":"tab","action":"focusNext","label":"focusNext","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(0)},
    {"mods":"shift","key":"Tab","keys":"shift-tab","action":"focusPrevious","label":"focusPrevious","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(Qt.ShiftModifier)},
    {"mods":"shift","key":"Backtab","keys":"shift-backtab","action":"focusPrevious","label":"focusPrevious","frontend":"all","context":"rail,menu,panel,preview,pdf,media","preset":"all","code":"Key_Backtab","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Backtab,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Right","keys":"right","action":"menuRight","label":"menuRight","frontend":"all","context":"menu","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"parent","label":"parent","frontend":"all","context":"menu","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"text","key":"l","keys":"l","action":"menuRight","label":"menuRight","frontend":"all","context":"menu","preset":"all","code":"","text":"l","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"h","keys":"h","action":"parent","label":"parent","frontend":"all","context":"menu","preset":"all","code":"","text":"h","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"ctrl","key":"Tab","keys":"ctrl-tab","action":"focusPreview","label":"focus listing","frontend":"all","context":"preview,pdf,media","preset":"all","code":"Key_Tab","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(Qt.ControlModifier)},
    {"mods":"none","key":"Left","keys":"left","action":"seekBack","label":"seekBack","frontend":"all","context":"preview,pdf,media","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Right","keys":"right","action":"seekForward","label":"seekForward","frontend":"all","context":"preview,pdf,media","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"text","key":"h","keys":"h","action":"seekBack","label":"seekBack","frontend":"all","context":"preview,pdf,media","preset":"all","code":"","text":"h","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"l","keys":"l","action":"pageForward","label":"pageForward","frontend":"all","context":"preview,pdf,media","preset":"all","code":"","text":"l","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"-","keys":"-","action":"zoomOut","label":"zoomOut","frontend":"all","context":"pdf","preset":"all","code":"","text":"-","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"+","keys":"+","action":"zoomIn","label":"zoomIn","frontend":"all","context":"pdf","preset":"all","code":"","text":"+","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"e","keys":"e","action":"expand","label":"expand","frontend":"all","context":"pdf","preset":"all","code":"","text":"e","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","label":"close editor","frontend":"all","context":"editor","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
]
var SHARED_KEYS = [
    {"mods":"ctrlshift","key":"N","keys":"ctrl-shift-n","action":"newFolder","context":"listing","frontend":"all","preset":"all","code":"Key_N","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_N,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Plus","keys":"ctrl-shift-plus","action":"textSizeUp","context":"listing","frontend":"all","preset":"all","code":"Key_Plus","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Plus,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Equal","keys":"ctrl-shift-equal","action":"textSizeUp","context":"listing","frontend":"all","preset":"all","code":"Key_Equal","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Equal,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Minus","keys":"ctrl-shift-minus","action":"textSizeDown","context":"listing","frontend":"all","preset":"all","code":"Key_Minus","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Minus,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Underscore","keys":"ctrl-shift-underscore","action":"textSizeDown","context":"listing","frontend":"all","preset":"all","code":"Key_Underscore","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Underscore,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"0","keys":"ctrl-shift-0","action":"textSizeReset","context":"listing","frontend":"all","preset":"all","code":"Key_0","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_0,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Greater","keys":"ctrl-shift-greater","action":"toggleHidden","context":"listing","frontend":"all","preset":"all","code":"Key_Greater","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Greater,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrlshift","key":"Period","keys":"ctrl-shift-.","action":"toggleHidden","context":"listing","frontend":"all","preset":"all","code":"Key_Period","text":"","ctrl":true,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Period,"mask":(Qt.ControlModifier | Qt.ShiftModifier)},
    {"mods":"ctrl","key":"D","keys":"ctrl-d","action":"pageDown","context":"listing","frontend":"all","preset":"all","code":"Key_D","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_D,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"U","keys":"ctrl-u","action":"pageUp","context":"listing","frontend":"all","preset":"all","code":"Key_U","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_U,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"A","keys":"ctrl-a","action":"selectAll","context":"listing","frontend":"all","preset":"all","code":"Key_A","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_A,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"C","keys":"ctrl-c","action":"copy","context":"listing","frontend":"all","preset":"all","code":"Key_C","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_C,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"V","keys":"ctrl-v","action":"paste","context":"listing","frontend":"all","preset":"all","code":"Key_V","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_V,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"X","keys":"ctrl-x","action":"cut","context":"listing","frontend":"all","preset":"all","code":"Key_X","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_X,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Z","keys":"ctrl-z","action":"undo","context":"listing","frontend":"all","preset":"all","code":"Key_Z","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Z,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"F","keys":"ctrl-f","action":"search","context":"listing","frontend":"all","preset":"all","code":"Key_F","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_F,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"E","keys":"ctrl-e","action":"eject","context":"listing","frontend":"all","preset":"all","code":"Key_E","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_E,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"L","keys":"ctrl-l","action":"pathBar","context":"listing","frontend":"all","preset":"all","code":"Key_L","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_L,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"T","keys":"ctrl-t","action":"openTerminal","context":"listing","frontend":"all","preset":"all","code":"Key_T","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_T,"mask":(Qt.ControlModifier)},
    {"mods":"ctrl","key":"Comma","keys":"ctrl-comma","action":"settings","context":"listing","frontend":"all","preset":"all","code":"Key_Comma","text":"","ctrl":true,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Comma,"mask":(Qt.ControlModifier)},
    {"mods":"shift","key":"Down","keys":"shift-down","action":"extendDown","context":"listing","frontend":"all","preset":"all","code":"Key_Down","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(Qt.ShiftModifier)},
    {"mods":"shift","key":"Up","keys":"shift-up","action":"extendUp","context":"listing","frontend":"all","preset":"all","code":"Key_Up","text":"","ctrl":false,"shift":true,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(Qt.ShiftModifier)},
    {"mods":"none","key":"Down","keys":"down","action":"cursorDown","context":"listing","frontend":"all","preset":"all","code":"Key_Down","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Down,"mask":(0)},
    {"mods":"none","key":"Up","keys":"up","action":"cursorUp","context":"listing","frontend":"all","preset":"all","code":"Key_Up","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Up,"mask":(0)},
    {"mods":"none","key":"Home","keys":"home","action":"cursorFirst","context":"listing","frontend":"all","preset":"all","code":"Key_Home","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Home,"mask":(0)},
    {"mods":"none","key":"End","keys":"end","action":"cursorLast","context":"listing","frontend":"all","preset":"all","code":"Key_End","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_End,"mask":(0)},
    {"mods":"none","key":"PageUp","keys":"pageup","action":"pageUp","context":"listing","frontend":"all","preset":"all","code":"Key_PageUp","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_PageUp,"mask":(0)},
    {"mods":"none","key":"PageDown","keys":"pagedown","action":"pageDown","context":"listing","frontend":"all","preset":"all","code":"Key_PageDown","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_PageDown,"mask":(0)},
    {"mods":"none","key":"Return","keys":"enter","action":"open","context":"listing","frontend":"all","preset":"all","code":"Key_Return","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Return,"mask":(0)},
    {"mods":"none","key":"Enter","keys":"enter","action":"open","context":"listing","frontend":"all","preset":"all","code":"Key_Enter","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Enter,"mask":(0)},
    {"mods":"none","key":"Backspace","keys":"backspace","action":"parent","context":"listing","frontend":"all","preset":"all","code":"Key_Backspace","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Backspace,"mask":(0)},
    {"mods":"none","key":"Delete","keys":"delete","action":"trash","context":"listing","frontend":"all","preset":"all","code":"Key_Delete","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Delete,"mask":(0)},
    {"mods":"none","key":"Escape","keys":"escape","action":"escape","context":"listing","frontend":"all","preset":"all","code":"Key_Escape","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Escape,"mask":(0)},
    {"mods":"none","key":"Tab","keys":"tab","action":"focusNext","context":"listing","frontend":"all","preset":"all","code":"Key_Tab","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Tab,"mask":(0)},
    {"mods":"none","key":"Space","keys":"space","action":"preview","context":"listing","frontend":"all","preset":"all","code":"Key_Space","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Space,"mask":(0)},
    {"mods":"none","key":"F2","keys":"f2","action":"rename","context":"listing","frontend":"all","preset":"all","code":"Key_F2","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_F2,"mask":(0)},
    {"mods":"none","key":"Left","keys":"left","action":"seekBack","context":"listing","frontend":"all","preset":"all","code":"Key_Left","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Left,"mask":(0)},
    {"mods":"none","key":"Right","keys":"right","action":"seekForward","context":"listing","frontend":"all","preset":"all","code":"Key_Right","text":"","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":Qt.Key_Right,"mask":(0)},
    {"mods":"text","key":"j","keys":"j","action":"cursorDown","context":"listing","frontend":"all","preset":"all","code":"","text":"j","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"k","keys":"k","action":"cursorUp","context":"listing","frontend":"all","preset":"all","code":"","text":"k","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"g","keys":"g","action":"cursorFirst","context":"listing","frontend":"all","preset":"all","code":"","text":"g","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"G","keys":"G","action":"cursorLast","context":"listing","frontend":"all","preset":"all","code":"","text":"G","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"v","keys":"v","action":"toggleSelect","context":"listing","frontend":"all","preset":"all","code":"","text":"v","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"J","keys":"J","action":"extendDown","context":"listing","frontend":"all","preset":"all","code":"","text":"J","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"K","keys":"K","action":"extendUp","context":"listing","frontend":"all","preset":"all","code":"","text":"K","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"h","keys":"h","action":"parent","context":"listing","frontend":"all","preset":"all","code":"","text":"h","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"/","keys":"/","action":"filter","context":"listing","frontend":"all","preset":"all","code":"","text":"/","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"f","keys":"f","action":"search","context":"listing","frontend":"all","preset":"all","code":"","text":"f","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"o","keys":"o","action":"reveal","context":"listing","frontend":"all","preset":"all","code":"","text":"o","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":":","keys":":","action":"pathBar","context":"listing","frontend":"all","preset":"all","code":"","text":":","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"t","keys":"t","action":"tabNew","context":"listing","frontend":"all","preset":"all","code":"","text":"t","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"w","keys":"w","action":"tabClose","context":"listing","frontend":"all","preset":"all","code":"","text":"w","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"y","keys":"y","action":"copy","context":"listing","frontend":"all","preset":"all","code":"","text":"y","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"Y","keys":"Y","action":"copydirpath","context":"listing","frontend":"all","preset":"all","code":"","text":"Y","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"x","keys":"x","action":"cut","context":"listing","frontend":"all","preset":"all","code":"","text":"x","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"p","keys":"p","action":"paste","context":"listing","frontend":"all","preset":"all","code":"","text":"p","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"d","keys":"dd","action":"trashArm","context":"listing","frontend":"all","preset":"all","code":"","text":"d","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"r","keys":"r","action":"rename","context":"listing","frontend":"all","preset":"all","code":"","text":"r","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"z","keys":"z","action":"undo","context":"listing","frontend":"all","preset":"all","code":"","text":"z","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"s","keys":"s","action":"sortNext","context":"listing","frontend":"all","preset":"all","code":"","text":"s","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"S","keys":"S","action":"sortReverse","context":"listing","frontend":"all","preset":"all","code":"","text":"S","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":".","keys":".","action":"toggleHidden","context":"listing","frontend":"all","preset":"all","code":"","text":".","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"a","keys":"a","action":"addNetwork","context":"listing","frontend":"all","preset":"all","code":"","text":"a","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"m","keys":"m","action":"menu","context":"listing","frontend":"all","preset":"all","code":"","text":"m","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":",","keys":",","action":"settings","context":"listing","frontend":"all","preset":"all","code":"","text":",","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"?","keys":"?","action":"keymapSheet","context":"listing","frontend":"all","preset":"all","code":"","text":"?","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"-","keys":"-","action":"zoomOut","context":"listing","frontend":"all","preset":"all","code":"","text":"-","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"+","keys":"+","action":"zoomIn","context":"listing","frontend":"all","preset":"all","code":"","text":"+","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"e","keys":"e","action":"expand","context":"listing","frontend":"all","preset":"all","code":"","text":"e","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
    {"mods":"text","key":"l","keys":"l","action":"pageForward","context":"listing","frontend":"all","preset":"all","code":"","text":"l","ctrl":false,"shift":false,"alt":false,"super":false,"keycode":0,"mask":(0)},
]
var POINTER = [{"where":"listing","press":"left","row":"any","does":"selectOnly","label":"put the cursor on the row and drop any other selection"},{"where":"listing","press":"left x2","row":"any","does":"open","label":"open the row"},{"where":"listing","press":"left","row":"result","does":"reveal","label":"go to the file in its own directory, selected"},{"where":"listing","press":"left x2","row":"result","does":"reveal","label":"still the one reveal the first tap made"},{"where":"listing","press":"ctrl left","row":"any","does":"toggleSelect","label":"add the row to the selection"},{"where":"listing","press":"shift left","row":"any","does":"extendSelect","label":"extend the selection to the row"},{"where":"listing","press":"ctrl left x2","row":"any","does":"toggleSelect","label":"still only selects"},{"where":"listing","press":"shift left x2","row":"any","does":"extendSelect","label":"still only selects"},{"where":"listing","press":"left","row":"renaming","does":"commitRename","label":"commit the open rename, then select the row"},{"where":"listing","press":"right","row":"any","does":"menu","label":"open the context menu at the pointer, on the selection the row is in"},{"where":"column","press":"left","row":"dir","does":"open","label":"go into that directory on one tap, as its neighbours do"},{"where":"neighbour","press":"left","row":"dir","does":"reveal","label":"show that directory in the middle column"},{"where":"neighbour","press":"left","row":"file","does":"nothing","label":"a file has no contents to reveal"},{"where":"neighbour","press":"left x2","row":"file","does":"open","label":"open the file"},{"where":"neighbour","press":"right","row":"any","does":"nothing","label":"a peeked row has no menu"},{"where":"chrome","press":"left","row":"parent","does":"goToCrumb","label":"open the directory that segment of the path names"},{"where":"chrome","press":"left x2","row":"any","does":"pathBar","label":"type the path instead of clicking it"},{"where":"window","press":"back","row":"any","does":"backOrParent","label":"go back through the history, or up a directory when there is none"},{"where":"rail","press":"left","row":"any","does":"open","label":"open the place"},{"where":"rail","press":"right","row":"any","does":"menu","label":"eject and unmount"}]
var BASE_SHEET = [{"keys":"j k","action":"cursorDown","label":"move"},{"keys":"enter","action":"open","label":"open"},{"keys":"l","action":"pageForward","label":"browse forward"},{"keys":"space","action":"preview","label":"preview"},{"keys":"/","action":"filter","label":"filter"},{"keys":"f","action":"search","label":"find in subtree"},{"keys":"o","action":"reveal","label":"reveal result"},{"keys":"tab","action":"focusNext","label":"search scope, or focus"},{"keys":": ^l","action":"pathBar","label":"go to path"},{"keys":"y ^c","action":"copy","label":"copy"},{"keys":"Y","action":"copydirpath","label":"copy folder path"},{"keys":"x ^x","action":"cut","label":"cut"},{"keys":"p ^v","action":"paste","label":"paste"},{"keys":"r","action":"rename","label":"rename"},{"keys":"dd","action":"trashArm","label":"trash"},{"keys":"z ^z","action":"undo","label":"undo"},{"keys":"^N","action":"newFolder","label":"new folder"},{"keys":"v","action":"toggleSelect","label":"select"},{"keys":"s","action":"sortNext","label":"sort column"},{"keys":"S","action":"sortReverse","label":"reverse sort"},{"keys":". ^>","action":"toggleHidden","label":"hidden files"},{"keys":"a","action":"addNetwork","label":"add network place"},{"keys":"m","action":"menu","label":"context menu"},{"keys":"^e","action":"eject","label":"eject"},{"keys":"^t","action":"openTerminal","label":"open terminal"},{"keys":"^+","action":"textSizeUp","label":"text size up"},{"keys":"^-","action":"textSizeDown","label":"text size down"},{"keys":",","action":"settings","label":"settings"},{"keys":"?","action":"keymapSheet","label":"this sheet"}]
var DIGITS = {"from":1,"to":9,"prefix":"tab"}

function applies(row, context, frontend) {
    return (row.context.split(",").indexOf(context) >= 0 || row.context === "all")
           && (row.frontend === frontend || row.frontend === "all")
}
function matches(row, key, text, modifiers) {
    var mask = modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
    if (row.mods === "text")
        return (mask & ~Qt.ShiftModifier) === 0 && text === row.text
    return mask === row.mask && key === row.keycode
}
// A matching empty action suppresses fallback, as Mac Ctrl+X requires.
function presetMatch(name, key, text, modifiers, context, frontend) {
    for (var pass = 0; pass < 2; pass++) {
        for (var i = 0; i < PRESET_KEYS.length; i++) {
            var row = PRESET_KEYS[i]
            if (row.preset !== (pass === 0 ? name : "all")) continue
            if (applies(row, context, frontend) && matches(row, key, text, modifiers)) return row
        }
    }
    return null
}
function lookupPreset(name, key, text, modifiers, context, frontend) {
    var row = presetMatch(name, key, text, modifiers, context || "listing", frontend || "gui")
    return row ? row.action : ""
}
function lookupFor(name, key, text, modifiers, context, frontend) {
    context = context || "listing"
    frontend = frontend || "gui"
    var row = presetMatch(name, key, text, modifiers, context, frontend)
    if (row) return row.action
    if (context !== "listing" && context !== "rail") return ""
    for (var i = 0; i < SHARED_KEYS.length; i++) {
        if (matches(SHARED_KEYS[i], key, text, modifiers)) return SHARED_KEYS[i].action
    }
    if (frontend === "tui" && modifiers === 0 && text >= String(DIGITS.from) && text <= String(DIGITS.to))
        return DIGITS.prefix + text
    return ""
}
function lookup(key, text, modifiers, context, frontend) {
    return lookupFor(preset, key, text, modifiers, context, frontend)
}
function actionGroup(action) {
    var arms = { copyArm: "copy", cutArm: "cut", pasteArm: "paste", cursorFirstArm: "cursorFirst", trashArm: "trash" }
    return arms[action] || action
}
function bindingRows(name, frontend) {
    var rows = [], candidates = PRESET_KEYS.concat(SHARED_KEYS)
    for (var i = 0; i < candidates.length; i++) {
        var row = candidates[i]
        if (row.preset !== "all" && row.preset !== name) continue
        if (!applies(row, "listing", frontend || "gui") || !row.action) continue
        if (lookupFor(name, row.keycode, row.text, row.mask, "listing", frontend || "gui") !== row.action) continue
        var duplicate = rows.some(function (kept) { return kept.keys === row.keys && kept.action === row.action })
        if (!duplicate) rows.push(row)
    }
    return rows
}
function hintFor(action) {
    return HINTS[action] || ""
}
// How wide a cap may get before a second spelling stops earning its place. The sheet draws two
// columns of a 300 unit card, so a cap past this elides and the wording beside it has nowhere to go.
var SHEET_CAP_BUDGET = 16

// An action id is not wording. A row the base sheet does not name printed its own identifier, so the
// pane advertised "pageDown" and "textSizeReset" beside sentences like "hidden files".
function spelledOut(action) {
    return String(action).replace(/([a-z0-9])([A-Z])/g, "$1 $2").toLowerCase()
}

// Which spelling speaks for an action: the preset's own before an inherited one, and a plain key
// before a chord. setPreset ranks the menu hint the same way, so the sheet and the menus agree.
function capRank(row, preset) {
    return (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
}

function sheetFor(name, frontend, dual) {
    var result = [], groups = {}, rows = bindingRows(name, frontend || "gui")
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i], action = actionGroup(row.action)
        var group = groups[action]
        if (!group) {
            var label = row.label || spelledOut(action)
            for (var j = 0; j < BASE_SHEET.length; j++)
                if (actionGroup(BASE_SHEET[j].action) === action) label = BASE_SHEET[j].label
            group = { action: action, label: label, keys: "", context: "listing", spellings: [] }
            result.push(group)
            groups[action] = group
        }
        group.spellings.push(row)
    }
    // One cap names one key. Joining every spelling an action answers to built caps of 40 characters
    // on the default preset and 78 on mac, wider than the whole card, so the pane drew them across
    // the column beside it. The best spelling always shows, a second only while both still fit.
    for (var g = 0; g < result.length; g++) {
        var spellings = result[g].spellings.slice()
        spellings.sort(function (left, right) { return capRank(left, name) - capRank(right, name) })
        var keys = spellings.length ? spellings[0].keys : ""
        for (var k = 1; k < spellings.length; k++) {
            var both = keys + " / " + spellings[k].keys
            if (spellings[k].keys === keys || both.length > SHEET_CAP_BUDGET) continue
            keys = both
            break
        }
        result[g].keys = keys
        delete result[g].spellings
    }
    if (dual && groups.focusNext) {
        groups.focusNext.keys = "tab"
        groups.focusNext.label = "focus other pane"
    }
    return result
}
function setPreset(name) {
    preset = PRESETS.indexOf(name) >= 0 ? name : "default"
    SHEET = sheetFor(preset, "gui")
    HINTS = {}
    var ranks = {}, rows = bindingRows(preset, "gui")
    for (var i = 0; i < rows.length; i++) {
        var row = rows[i]
        if (row.mods !== "text" && row.mods !== "none") continue
        var action = actionGroup(row.action), rank = (row.preset === preset ? 0 : 2) + (row.mods === "text" ? 0 : 1)
        if (ranks[action] !== undefined && ranks[action] <= rank) continue
        // A menu hint is the key the operator presses: Menus.html and the OpenWith overseer board
        // both draw Move to Trash with d. The full dd chord stays on the keymap sheet below.
        HINTS[action] = row.mods === "text" ? row.key : row.keys
        ranks[action] = rank
    }
}
var SHEET = []
var HINTS = {}
setPreset("default")
