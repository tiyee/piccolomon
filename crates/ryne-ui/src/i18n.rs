//! UI text is presentation state, never document content or history.
use std::{
    io,
    path::{Path, PathBuf},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(crate) enum Language {
    ZhHans,
    ZhHant,
    En,
    Ja,
    Ko,
    Fr,
    Es,
}
impl Language {
    pub const ALL: [Self; 7] = [
        Self::ZhHans,
        Self::ZhHant,
        Self::En,
        Self::Ja,
        Self::Ko,
        Self::Fr,
        Self::Es,
    ];
    pub fn code(self) -> &'static str {
        ["zh-Hans", "zh-Hant", "en", "ja", "ko", "fr", "es"][self as usize]
    }
    pub fn native_name(self) -> &'static str {
        [
            "简体中文",
            "繁體中文",
            "English",
            "日本語",
            "한국어",
            "Français",
            "Español",
        ][self as usize]
    }
    pub fn parse(value: &str) -> Option<Self> {
        let normalized = value.trim().replace('_', "-").to_ascii_lowercase();
        let normalized = normalized.split(['.', '@']).next()?;
        let parts: Vec<_> = normalized.split('-').collect();
        match *parts.first()? {
            "zh" if parts.contains(&"hant") => Some(Self::ZhHant),
            "zh" if parts.contains(&"hans") => Some(Self::ZhHans),
            "zh" if parts.iter().any(|p| ["tw", "hk", "mo"].contains(p)) => Some(Self::ZhHant),
            "zh" => Some(Self::ZhHans),
            "en" => Some(Self::En),
            "ja" => Some(Self::Ja),
            "ko" => Some(Self::Ko),
            "fr" => Some(Self::Fr),
            "es" => Some(Self::Es),
            _ => None,
        }
    }
    pub fn text(self, key: Text) -> &'static str {
        key.translations()[self as usize]
    }
    pub fn render_label(self, key: &'static str, fallback: &'static str) -> &'static str {
        let text = match key {
            "table.alignment" => Text::Alignment,
            "table.align-left" => Text::AlignLeft,
            "table.align-center" => Text::AlignCenter,
            "table.align-right" => Text::AlignRight,
            "table.insert-below" => Text::InsertRowBelow,
            "table.insert-above" => Text::InsertRowAbove,
            "table.insert-right" => Text::InsertColumnAfter,
            "table.insert-left" => Text::InsertColumnBefore,
            "table.move-row-up" => Text::MoveRowUp,
            "table.move-row-down" => Text::MoveRowDown,
            "table.move-column-left" => Text::MoveColumnLeft,
            "table.move-column-right" => Text::MoveColumnRight,
            "table.delete-row" => Text::DeleteRow,
            "table.delete-column" => Text::DeleteColumn,
            "list.insert-after" => Text::InsertItemAfter,
            "list.insert-before" => Text::InsertItemBefore,
            "list.move-up" => Text::MoveItemUp,
            "list.move-down" => Text::MoveItemDown,
            "list.indent" => Text::IndentItem,
            "list.outdent" => Text::OutdentItem,
            "list.delete" => Text::DeleteItem,
            _ => return fallback,
        };
        self.text(text)
    }
    pub fn error(self, key: Text, detail: impl std::fmt::Display) -> Status {
        Status {
            key,
            detail: Some(detail.to_string()),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Status {
    key: Text,
    detail: Option<String>,
}
impl From<Text> for Status {
    fn from(key: Text) -> Self {
        Self { key, detail: None }
    }
}
impl Status {
    pub fn render(&self, language: Language) -> String {
        let text = language.text(self.key);
        self.detail
            .as_ref()
            .map_or_else(|| text.into(), |detail| format!("{text}: {detail}"))
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) enum Text {
    Open,
    Save,
    Cancel,
    Discard,
    DiscardOpen,
    OpenOther,
    Unsaved,
    SaveChanges,
    OpenMarkdown,
    Untitled,
    Source,
    Preview,
    BlockSource,
    FoldSource,
    EditSource,
    SelectBlock,
    CopyBlock,
    DeleteRow,
    DeleteColumn,
    InsertRowBelow,
    Alignment,
    AlignLeft,
    AlignCenter,
    AlignRight,
    InsertRowAbove,
    InsertColumnAfter,
    InsertColumnBefore,
    MoveRowUp,
    MoveRowDown,
    MoveColumnLeft,
    MoveColumnRight,
    InsertItemAfter,
    InsertItemBefore,
    MoveItemUp,
    MoveItemDown,
    IndentItem,
    OutdentItem,
    DeleteItem,
    RenderActionFailed,
    SourceMode,
    BlockMode,
    LivePreview,
    Language,
    System,
    Help,
    Added,
    Copied,
    PlaceCaret,
    SavingWait,
    Saving,
    Saved,
    SavedEarlier,
    Opened,
    ChangedWhileOpening,
    OpenFailed,
    SaveFailed,
    InputFailed,
    DeleteFailed,
    BreakFailed,
    IndentFailed,
    OutdentFailed,
    FormulaFailed,
    DiagramFailed,
    CompositionRange,
    AppendFailed,
    PreferenceFailed,
    AddContent,
    Paragraph,
    Heading,
    Unordered,
    Ordered,
    Task,
    Quote,
    Code,
    Table,
    Image,
    Formula,
    Rule,
    Raw,
    Body,
    ListItem,
    Todo,
    ColumnOne,
    ColumnTwo,
    ImageDescription,
    ImageUrl,
    Start,
    End,
}
impl Text {
    #[cfg(test)]
    const ALL: [Self; 88] = [
        Self::Open,
        Self::Save,
        Self::Cancel,
        Self::Discard,
        Self::DiscardOpen,
        Self::OpenOther,
        Self::Unsaved,
        Self::SaveChanges,
        Self::OpenMarkdown,
        Self::Untitled,
        Self::Source,
        Self::Preview,
        Self::BlockSource,
        Self::FoldSource,
        Self::EditSource,
        Self::SelectBlock,
        Self::CopyBlock,
        Self::DeleteRow,
        Self::DeleteColumn,
        Self::InsertRowBelow,
        Self::Alignment,
        Self::AlignLeft,
        Self::AlignCenter,
        Self::AlignRight,
        Self::InsertRowAbove,
        Self::InsertColumnAfter,
        Self::InsertColumnBefore,
        Self::MoveRowUp,
        Self::MoveRowDown,
        Self::MoveColumnLeft,
        Self::MoveColumnRight,
        Self::InsertItemAfter,
        Self::InsertItemBefore,
        Self::MoveItemUp,
        Self::MoveItemDown,
        Self::IndentItem,
        Self::OutdentItem,
        Self::DeleteItem,
        Self::RenderActionFailed,
        Self::SourceMode,
        Self::BlockMode,
        Self::LivePreview,
        Self::Language,
        Self::System,
        Self::Help,
        Self::Added,
        Self::Copied,
        Self::PlaceCaret,
        Self::SavingWait,
        Self::Saving,
        Self::Saved,
        Self::SavedEarlier,
        Self::Opened,
        Self::ChangedWhileOpening,
        Self::OpenFailed,
        Self::SaveFailed,
        Self::InputFailed,
        Self::DeleteFailed,
        Self::BreakFailed,
        Self::IndentFailed,
        Self::OutdentFailed,
        Self::FormulaFailed,
        Self::DiagramFailed,
        Self::CompositionRange,
        Self::AppendFailed,
        Self::PreferenceFailed,
        Self::AddContent,
        Self::Paragraph,
        Self::Heading,
        Self::Unordered,
        Self::Ordered,
        Self::Task,
        Self::Quote,
        Self::Code,
        Self::Table,
        Self::Image,
        Self::Formula,
        Self::Rule,
        Self::Raw,
        Self::Body,
        Self::ListItem,
        Self::Todo,
        Self::ColumnOne,
        Self::ColumnTwo,
        Self::ImageDescription,
        Self::ImageUrl,
        Self::Start,
        Self::End,
    ];

    fn translations(self) -> [&'static str; 7] {
        match self {
            Self::InsertItemAfter => ["在后面添加条目", "在後面新增項目", "Insert item after", "後に項目を追加", "뒤에 항목 추가", "Insérer un élément après", "Insertar elemento después"],
            Self::InsertItemBefore => ["在前面添加条目", "在前面新增項目", "Insert item before", "前に項目を追加", "앞에 항목 추가", "Insérer un élément avant", "Insertar elemento antes"],
            Self::MoveItemUp => ["上移条目", "上移項目", "Move item up", "項目を上に移動", "항목 위로 이동", "Monter l’élément", "Subir elemento"],
            Self::MoveItemDown => ["下移条目", "下移項目", "Move item down", "項目を下に移動", "항목 아래로 이동", "Descendre l’élément", "Bajar elemento"],
            Self::IndentItem => ["缩进", "縮排", "Indent", "インデント", "들여쓰기", "Augmenter le retrait", "Aumentar sangría"],
            Self::OutdentItem => ["反缩进", "減少縮排", "Outdent", "インデントを減らす", "내어쓰기", "Réduire le retrait", "Reducir sangría"],
            Self::DeleteItem => ["删除条目及子项", "刪除項目及子項", "Delete item and children", "項目と子項目を削除", "항목 및 하위 항목 삭제", "Supprimer l’élément et ses enfants", "Eliminar elemento y subelementos"],
            Self::DeleteRow => ["删除此行", "刪除此列", "Delete row", "行を削除", "행 삭제", "Supprimer la ligne", "Eliminar fila"],
            Self::DeleteColumn => ["删除此列", "刪除此欄", "Delete column", "列を削除", "열 삭제", "Supprimer la colonne", "Eliminar columna"],
            Self::InsertRowBelow => ["添加行", "新增列", "Insert row below", "下に行を追加", "아래에 행 추가", "Insérer une ligne en dessous", "Insertar fila debajo"],
            Self::Alignment => ["对齐方式", "對齊方式", "Alignment", "配置", "정렬", "Alignement", "Alineación"],
            Self::AlignLeft => ["靠左", "靠左", "Left", "左揃え", "왼쪽", "À gauche", "Izquierda"],
            Self::AlignCenter => ["居中", "置中", "Center", "中央揃え", "가운데", "Centré", "Centro"],
            Self::AlignRight => ["右边", "靠右", "Right", "右揃え", "오른쪽", "À droite", "Derecha"],
            Self::InsertRowAbove => ["在上面添加行", "在上方新增列", "Insert row above", "上に行を追加", "위에 행 추가", "Insérer une ligne au-dessus", "Insertar fila encima"],
            Self::InsertColumnAfter => ["添加列", "新增欄", "Insert column after", "右に列を追加", "오른쪽에 열 추가", "Insérer une colonne après", "Insertar columna después"],
            Self::InsertColumnBefore => ["在前面添加列", "在前面新增欄", "Insert column before", "左に列を追加", "왼쪽에 열 추가", "Insérer une colonne avant", "Insertar columna antes"],
            Self::MoveRowUp => ["上移一行", "上移一列", "Move row up", "行を上に移動", "행 위로 이동", "Monter la ligne", "Subir fila"],
            Self::MoveRowDown => ["下移一行", "下移一列", "Move row down", "行を下に移動", "행 아래로 이동", "Descendre la ligne", "Bajar fila"],
            Self::MoveColumnLeft => ["向左移动列", "向左移動欄", "Move column left", "列を左に移動", "열 왼쪽으로 이동", "Déplacer la colonne à gauche", "Mover columna a la izquierda"],
            Self::MoveColumnRight => ["向右移动列", "向右移動欄", "Move column right", "列を右に移動", "열 오른쪽으로 이동", "Déplacer la colonne à droite", "Mover columna a la derecha"],
            Self::RenderActionFailed => ["操作失败", "操作失敗", "Action failed", "操作に失敗しました", "작업 실패", "Échec de l’action", "La acción falló"],
            Self::Open => ["打开", "開啟", "Open", "開く", "열기", "Ouvrir", "Abrir"],
            Self::Save => ["保存", "儲存", "Save", "保存", "저장", "Enregistrer", "Guardar"],
            Self::Cancel => ["取消", "取消", "Cancel", "キャンセル", "취소", "Annuler", "Cancelar"],
            Self::Discard => ["放弃修改", "捨棄修改", "Discard changes", "変更を破棄", "변경 사항 버리기", "Ignorer les modifications", "Descartar cambios"],
            Self::DiscardOpen => ["放弃修改并打开", "捨棄修改並開啟", "Discard and open", "破棄して開く", "버리고 열기", "Ignorer et ouvrir", "Descartar y abrir"],
            Self::OpenOther => ["打开其他文件？", "開啟其他檔案？", "Open another file?", "別のファイルを開きますか？", "다른 파일을 열까요?", "Ouvrir un autre fichier ?", "¿Abrir otro archivo?"],
            Self::Unsaved => ["当前文档有未保存的修改。取消后可先保存。", "目前文件有未儲存的修改。取消後可先儲存。", "This document has unsaved changes. Cancel to save them first.", "未保存の変更があります。先に保存するにはキャンセルしてください。", "저장하지 않은 변경 사항이 있습니다. 먼저 저장하려면 취소하세요.", "Ce document contient des modifications non enregistrées. Annulez pour les enregistrer.", "Hay cambios sin guardar. Cancela para guardarlos primero."],
            Self::SaveChanges => ["保存修改？", "儲存修改？", "Save changes?", "変更を保存しますか？", "변경 사항을 저장할까요?", "Enregistrer les modifications ?", "¿Guardar los cambios?"],
            Self::OpenMarkdown => ["打开 Markdown 文件", "開啟 Markdown 檔案", "Open Markdown file", "Markdown ファイルを開く", "Markdown 파일 열기", "Ouvrir un fichier Markdown", "Abrir archivo Markdown"],
            Self::Untitled => ["未命名.md", "未命名.md", "Untitled.md", "無題.md", "제목 없음.md", "Sans titre.md", "Sin título.md"],
            Self::Source => ["源码", "原始碼", "Source", "ソース", "소스", "Source", "Código fuente"],
            Self::Preview => ["返回预览", "返回預覽", "Preview", "プレビュー", "미리 보기", "Aperçu", "Vista previa"],
            Self::BlockSource => ["块源码", "區塊原始碼", "Block source", "ブロックソース", "블록 소스", "Source du bloc", "Código del bloque"],
            Self::FoldSource => ["收起块源码", "收起區塊原始碼", "Close block source", "ブロックソースを閉じる", "블록 소스 닫기", "Fermer la source du bloc", "Cerrar código del bloque"],
            Self::EditSource => ["编辑块源码", "編輯區塊原始碼", "Edit block source", "ブロックソースを編集", "블록 소스 편집", "Modifier la source du bloc", "Editar código del bloque"],
            Self::SelectBlock => ["选择整块", "選取整個區塊", "Select block", "ブロックを選択", "블록 선택", "Sélectionner le bloc", "Seleccionar bloque"],
            Self::CopyBlock => ["复制块源码", "複製區塊原始碼", "Copy block source", "ブロックソースをコピー", "블록 소스 복사", "Copier la source du bloc", "Copiar código del bloque"],
            Self::SourceMode => ["源码模式", "原始碼模式", "Source mode", "ソースモード", "소스 모드", "Mode source", "Modo código fuente"],
            Self::BlockMode => ["块源码模式", "區塊原始碼模式", "Block source mode", "ブロックソースモード", "블록 소스 모드", "Mode source du bloc", "Modo código del bloque"],
            Self::LivePreview => ["行内实时预览", "行內即時預覽", "Live preview", "ライブプレビュー", "실시간 미리 보기", "Aperçu en direct", "Vista previa en vivo"],
            Self::Language => ["语言", "語言", "Language", "言語", "언어", "Langue", "Idioma"],
            Self::System => ["跟随系统", "跟隨系統", "System default", "システム設定", "시스템 기본값", "Langue du système", "Idioma del sistema"],
            Self::Help => ["输入 # 后接正文生成标题 · 点击标题显示标记", "輸入 # 後接內文產生標題 · 點擊標題顯示標記", "Type # then text for a heading · Click a heading to reveal markers", "# と本文で見出しを作成 · クリックで記号を表示", "# 뒤에 텍스트를 입력하면 제목 생성 · 클릭하면 기호 표시", "Tapez # puis du texte pour un titre · Cliquez pour afficher les marqueurs", "Escribe # y texto para un título · Haz clic para ver los marcadores"],
            Self::Added => ["已添加内容 · 可直接输入替换选中文字", "已新增內容 · 可直接輸入取代所選文字", "Content added · Type to replace the selection", "追加しました · 入力すると選択部分を置換", "추가됨 · 입력하여 선택한 텍스트 바꾸기", "Contenu ajouté · Saisissez pour remplacer la sélection", "Contenido añadido · Escribe para reemplazar la selección"],
            Self::Copied => ["已复制块源码", "已複製區塊原始碼", "Block source copied", "ブロックソースをコピーしました", "블록 소스 복사됨", "Source du bloc copiée", "Código del bloque copiado"],
            Self::PlaceCaret => ["将光标放入内容块后，可编辑结构源码", "將游標放入內容區塊後，可編輯結構原始碼", "Place the caret in a block to edit its source", "ブロック内にカーソルを置いてソースを編集", "블록 안에 커서를 놓으면 소스 편집 가능", "Placez le curseur dans un bloc pour modifier sa source", "Coloca el cursor en un bloque para editar su código"],
            Self::SavingWait => ["正在保存，请等待保存完成", "正在儲存，請等待完成", "Saving; please wait", "保存中です。お待ちください", "저장 중입니다. 기다려 주세요", "Enregistrement en cours, veuillez patienter", "Guardando, espera por favor"],
            Self::Saving => ["正在保存…", "正在儲存…", "Saving…", "保存中…", "저장 중…", "Enregistrement…", "Guardando…"],
            Self::Saved => ["已保存", "已儲存", "Saved", "保存しました", "저장됨", "Enregistré", "Guardado"],
            Self::SavedEarlier => ["已保存先前版本，当前修改尚未保存", "已儲存先前版本，目前修改尚未儲存", "Earlier version saved; current changes are unsaved", "以前の版を保存しました。現在の変更は未保存です", "이전 버전이 저장됨. 현재 변경 사항은 저장되지 않음", "Version précédente enregistrée ; modifications actuelles non enregistrées", "Versión anterior guardada; los cambios actuales siguen sin guardar"],
            Self::Opened => ["文件已打开", "檔案已開啟", "File opened", "ファイルを開きました", "파일 열림", "Fichier ouvert", "Archivo abierto"],
            Self::ChangedWhileOpening => ["读取期间文档已更改，请重新打开文件", "讀取期間文件已變更，請重新開啟", "Document changed while opening; open the file again", "読み込み中に文書が変更されました。もう一度開いてください", "여는 동안 문서가 변경되었습니다. 다시 열어 주세요", "Document modifié pendant la lecture ; ouvrez-le à nouveau", "El documento cambió durante la lectura; vuelve a abrirlo"],
            Self::OpenFailed => ["打开失败", "開啟失敗", "Could not open file", "ファイルを開けませんでした", "파일 열기 실패", "Impossible d’ouvrir le fichier", "No se pudo abrir el archivo"],
            Self::SaveFailed => ["保存失败", "儲存失敗", "Could not save file", "保存できませんでした", "저장 실패", "Impossible d’enregistrer", "No se pudo guardar"],
            Self::InputFailed => ["输入失败", "輸入失敗", "Input failed", "入力できませんでした", "입력 실패", "Échec de la saisie", "Error de entrada"],
            Self::DeleteFailed => ["删除失败", "刪除失敗", "Delete failed", "削除できませんでした", "삭제 실패", "Échec de la suppression", "Error al eliminar"],
            Self::BreakFailed => ["换行失败", "換行失敗", "Could not insert line break", "改行できませんでした", "줄 바꿈 실패", "Impossible d’insérer un saut de ligne", "No se pudo insertar un salto de línea"],
            Self::IndentFailed => ["缩进失败", "縮排失敗", "Indent failed", "インデントできませんでした", "들여쓰기 실패", "Échec de l’indentation", "Error de sangría"],
            Self::OutdentFailed => ["反缩进失败", "減少縮排失敗", "Outdent failed", "インデントを解除できませんでした", "내어쓰기 실패", "Échec de la désindentation", "Error al reducir sangría"],
            Self::FormulaFailed => ["公式绘制失败", "公式繪製失敗", "Formula drawing failed", "数式を描画できませんでした", "수식 그리기 실패", "Échec du dessin de la formule", "Error al dibujar la fórmula"],
            Self::DiagramFailed => ["图表绘制失败", "圖表繪製失敗", "Diagram drawing failed", "図を描画できませんでした", "다이어그램 그리기 실패", "Échec du dessin du diagramme", "Error al dibujar el diagrama"],
            Self::CompositionRange => ["组合替换超出当前范围，请先确认或取消输入", "組字取代超出目前範圍，請先確認或取消輸入", "Composition is outside the current range; confirm or cancel it first", "変換範囲外です。入力を確定またはキャンセルしてください", "조합 입력 범위를 벗어났습니다. 먼저 확정하거나 취소하세요", "Composition hors limites ; validez ou annulez d’abord la saisie", "La composición está fuera del rango; confírmala o cancélala primero"],
            Self::AppendFailed => ["添加失败，请检查文档末尾是否有未闭合结构", "新增失敗，請檢查文件末尾是否有未閉合結構", "Could not add content; check for unclosed structures at the end", "追加できませんでした。末尾の構造が閉じているか確認してください", "추가하지 못했습니다. 문서 끝에 닫히지 않은 구조가 있는지 확인하세요", "Ajout impossible ; vérifiez les structures non fermées à la fin", "No se pudo añadir; comprueba las estructuras sin cerrar al final"],
            Self::PreferenceFailed => ["语言已切换，但无法记住选择", "語言已切換，但無法記住選擇", "Language changed, but could not save the preference", "言語を変更しましたが、設定を保存できませんでした", "언어가 변경되었지만 설정을 저장하지 못했습니다", "Langue modifiée, mais préférence non enregistrée", "Idioma cambiado, pero no se pudo guardar la preferencia"],
            Self::AddContent => ["添加内容", "新增內容", "Add content", "コンテンツを追加", "콘텐츠 추가", "Ajouter du contenu", "Añadir contenido"],
            Self::Paragraph => ["段落", "段落", "Paragraph", "段落", "문단", "Paragraphe", "Párrafo"],
            Self::Heading => ["标题", "標題", "Heading", "見出し", "제목", "Titre", "Título"],
            Self::Unordered => ["无序列表", "無序清單", "Bullet list", "箇条書き", "글머리 기호 목록", "Liste à puces", "Lista con viñetas"],
            Self::Ordered => ["有序列表", "有序清單", "Numbered list", "番号付きリスト", "번호 목록", "Liste numérotée", "Lista numerada"],
            Self::Task => ["任务列表", "工作清單", "Task list", "タスクリスト", "작업 목록", "Liste de tâches", "Lista de tareas"],
            Self::Quote => ["引用", "引言", "Quote", "引用", "인용", "Citation", "Cita"],
            Self::Code => ["代码块", "程式碼區塊", "Code block", "コードブロック", "코드 블록", "Bloc de code", "Bloque de código"],
            Self::Table => ["表格", "表格", "Table", "表", "표", "Tableau", "Tabla"],
            Self::Image => ["图片", "圖片", "Image", "画像", "이미지", "Image", "Imagen"],
            Self::Formula => ["行内公式", "行內公式", "Inline math", "インライン数式", "인라인 수식", "Formule en ligne", "Fórmula en línea"],
            Self::Rule => ["分隔线", "分隔線", "Divider", "区切り線", "구분선", "Séparateur", "Separador"],
            Self::Raw => ["原始块", "原始區塊", "Raw block", "生のブロック", "원시 블록", "Bloc brut", "Bloque sin formato"],
            Self::Body => ["文本", "文字", "Text", "テキスト", "텍스트", "Texte", "Texto"],
            Self::ListItem => ["列表项", "清單項目", "List item", "リスト項目", "목록 항목", "Élément de liste", "Elemento de lista"],
            Self::Todo => ["待办事项", "待辦事項", "To-do", "タスク", "할 일", "Tâche", "Tarea"],
            Self::ColumnOne => ["列一", "欄一", "Column 1", "列 1", "열 1", "Colonne 1", "Columna 1"],
            Self::ColumnTwo => ["列二", "欄二", "Column 2", "列 2", "열 2", "Colonne 2", "Columna 2"],
            Self::ImageDescription => ["图片说明", "圖片說明", "Image description", "画像の説明", "이미지 설명", "Description de l’image", "Descripción de la imagen"],
            Self::ImageUrl => ["图片地址", "圖片網址", "image-url", "画像URL", "이미지URL", "url-image", "url-imagen"],
            Self::Start => ["开始", "開始", "Start", "開始", "시작", "Début", "Inicio"],
            Self::End => ["结束", "結束", "End", "終了", "끝", "Fin", "Fin"],
        }
    }
}

/// None means follow the OS. Store only an explicit language choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Preference(pub Option<Language>);
impl Preference {
    pub fn read(path: &Path) -> Self {
        Self(
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| Language::parse(&s)),
        )
    }
    pub fn write(self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result = std::fs::write(&temporary, self.0.map_or("system", Language::code))
            .and_then(|()| std::fs::rename(&temporary, path));
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
    pub fn resolve(self, system: Language) -> Language {
        self.0.unwrap_or(system)
    }
}
pub(crate) fn preference_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    #[cfg(target_os = "macos")]
    let base = PathBuf::from(home).join("Library/Application Support");
    #[cfg(not(target_os = "macos"))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".config"));
    Some(base.join("Ryne/language"))
}
pub(crate) fn system_language() -> Language {
    // Launch Services does not normally set LANG for GUI applications.
    #[cfg(target_os = "macos")]
    if let Ok(output) = std::process::Command::new("/usr/bin/defaults")
        .args(["read", "-g", "AppleLanguages"])
        .output()
        && output.status.success()
        && let Some(language) = String::from_utf8_lossy(&output.stdout)
            .lines()
            .find_map(|line| Language::parse(line.trim().trim_matches(['"', ',', ' '])))
    {
        return language;
    }
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()))
        .and_then(|s| Language::parse(&s))
        .unwrap_or(Language::En)
}

pub(crate) struct Localization {
    pub language: Language,
    pub preference: Preference,
    system: Language,
    path: Option<PathBuf>,
}
impl Localization {
    pub fn for_smoke() -> Self {
        Self {
            language: Language::ZhHans,
            preference: Preference(Some(Language::ZhHans)),
            system: Language::En,
            path: Some(
                std::env::temp_dir().join(format!("ryne-language-smoke-{}", std::process::id())),
            ),
        }
    }
    pub fn load() -> Self {
        let path = preference_path();
        let preference = path
            .as_deref()
            .map(Preference::read)
            .unwrap_or(Preference(None));
        let system = system_language();
        Self {
            language: preference.resolve(system),
            preference,
            system,
            path,
        }
    }
    pub fn select(&mut self, preference: Preference) -> io::Result<()> {
        self.preference = preference;
        self.language = preference.resolve(self.system);
        self.path
            .as_deref()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No configuration directory"))
            .and_then(|path| preference.write(path))
    }
}
impl Language {
    pub fn block_name(self, icon: Option<&str>, fallback: &'static str) -> &'static str {
        let key = match icon {
            Some("text_t") => Text::Paragraph,
            Some(
                "text_header_1" | "text_header_2" | "text_header_3" | "text_header_4"
                | "text_header_5" | "text_header_6",
            ) => Text::Heading,
            Some("text_bullet_list_ltr") => Text::Unordered,
            Some("text_number_list_ltr") => Text::Ordered,
            Some("task_list_ltr") => Text::Task,
            Some("text_quote") => Text::Quote,
            Some("code") if fallback == "原始块" => Text::Raw,
            Some("code") => Text::Code,
            Some("table") => Text::Table,
            Some("image") => Text::Image,
            Some("divider") => Text::Rule,
            _ => return fallback,
        };
        self.text(key)
    }
    pub fn insert_label(self, kind: ryne_extensions::InsertBlock) -> String {
        use ryne_extensions::InsertBlock as B;
        let (symbol, key) = match kind {
            B::Paragraph => ("¶", Text::Paragraph),
            B::Heading(level) => return format!("H{level}  {}", self.text(Text::Heading)),
            B::UnorderedList => ("•", Text::Unordered),
            B::OrderedList => ("1.", Text::Ordered),
            B::TaskList => ("☐", Text::Task),
            B::Quote => ("❯", Text::Quote),
            B::Code => ("</>", Text::Code),
            B::Mermaid => return "◇  Mermaid".into(),
            B::Table => ("▦", Text::Table),
            B::Image => ("IMG", Text::Image),
            B::Formula => ("ƒ", Text::Formula),
            B::Rule => ("—", Text::Rule),
        };
        format!("{symbol}  {}", self.text(key))
    }
    pub fn template_labels(self) -> ryne_extensions::TemplateLabels {
        use self::Text::*;
        ryne_extensions::TemplateLabels {
            text: self.text(Body),
            heading: self.text(Heading),
            list_item: self.text(ListItem),
            todo: self.text(Todo),
            quote: self.text(Quote),
            start: self.text(Start),
            end: self.text(End),
            column_one: self.text(ColumnOne),
            column_two: self.text(ColumnTwo),
            image_description: self.text(ImageDescription),
            image_url: self.text(ImageUrl),
        }
    }
    pub fn sample(self) -> String {
        let t = |key| self.text(key);
        format!("# Ryne\n\n{}\n\n**{}** · *{}* · ~~{}~~ · `inline code` · [{}](https://example.org)\n\n- {}\n  - {}\n\n1. {}\n2. Tab / Shift+Tab\n\n| {} | {} |\n| :--- | ---: |\n| Markdown | 123 |\n\n```rust\nfn main() {{\n    println!(\"Hello, Markdown\");\n}}\n```\n\n```mermaid\nflowchart LR\n    A[{}] --> B[Markdown] --> C[{}]\n```\n\n$E = mc^2$ · $\\frac{{a_1}}{{\\sqrt{{b}}}}$\n\nCmd+O · Cmd+S · Cmd+Z · Cmd+Shift+M\n\n", t(Text::Help), t(Text::Body), t(Text::Body), t(Text::Body), t(Text::Body), t(Text::ListItem), t(Text::ListItem), t(Text::ListItem), t(Text::ColumnOne), t(Text::ColumnTwo), t(Text::Open), t(Text::Save))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn temporary_directory() -> PathBuf {
        std::env::temp_dir().join(format!(
            "ryne-i18n-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
    #[test]
    fn locale_scripts_regions_and_posix_encodings_resolve() {
        for (value, expected) in [
            ("zh_CN.UTF-8", Language::ZhHans),
            ("zh-SG", Language::ZhHans),
            ("zh-TW", Language::ZhHant),
            ("zh_HK", Language::ZhHant),
            ("zh-MO", Language::ZhHant),
            ("zh-Hans-TW", Language::ZhHans),
            ("zh-Hant-CN", Language::ZhHant),
            ("en_GB.UTF-8", Language::En),
            ("ja-JP", Language::Ja),
            ("ko_KR", Language::Ko),
            ("fr-CA", Language::Fr),
            ("es-MX", Language::Es),
        ] {
            assert_eq!(Language::parse(value), Some(expected), "{value}");
        }
        for value in ["", "C", "C.UTF-8", "de-DE", "system", "broken"] {
            assert_eq!(Language::parse(value), None);
        }
        assert_eq!(Preference(None).resolve(Language::Ja), Language::Ja);
        assert_eq!(
            Preference(Some(Language::Es)).resolve(Language::Ja),
            Language::Es
        );
    }
    #[test]
    fn all_seven_catalogs_and_localized_status_are_complete() {
        for language in Language::ALL {
            assert_eq!(Language::parse(language.code()), Some(language));
            for key in Text::ALL {
                assert!(
                    !language.text(key).trim().is_empty(),
                    "{language:?}: {key:?}"
                );
            }
            assert!(language.sample().contains(language.text(Text::Help)));
            assert!(language
                .insert_label(ryne_extensions::InsertBlock::Heading(6))
                .starts_with("H6"));
        }
        assert_eq!(Language::Fr.text(Text::Save), "Enregistrer");
        assert_eq!(Language::ZhHant.text(Text::Save), "儲存");
        let error = Language::En.error(Text::OpenFailed, "/tmp/文件😀.md");
        assert_eq!(
            error.render(Language::Ja),
            "ファイルを開けませんでした: /tmp/文件😀.md"
        );
    }
    #[test]
    fn preferences_round_trip_reset_and_fail_without_losing_current_language() {
        let directory = temporary_directory();
        let path = directory.join("Ryne/language");
        assert_eq!(Preference::read(&path), Preference(None));
        for language in Language::ALL {
            let preference = Preference(Some(language));
            preference.write(&path).unwrap();
            assert_eq!(Preference::read(&path), preference);
        }
        Preference(None).write(&path).unwrap();
        assert_eq!(Preference::read(&path), Preference(None));
        std::fs::write(&path, "unsupported").unwrap();
        assert_eq!(Preference::read(&path), Preference(None));
        let mut locale = Localization {
            language: Language::En,
            preference: Preference(None),
            system: Language::Ja,
            path: Some(path.join("child")),
        };
        assert!(locale.select(Preference(Some(Language::Ko))).is_err());
        assert_eq!(locale.language, Language::Ko);
        assert!(locale.select(Preference(None)).is_err());
        assert_eq!(locale.language, Language::Ja);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
