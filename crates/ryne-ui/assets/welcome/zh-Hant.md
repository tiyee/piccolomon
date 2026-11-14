# 歡迎使用 Ryne（如言）

這是一份可以直接編輯的格式指南，介紹目前可用的格式和操作。點選文字即可編輯；游標進入行內格式時顯示 Markdown 標記，離開後收起。僅懸停標題不會展開標記。

在頂部選單列的 Ryne → 語言中切換語言，尚未編輯的預設指南也會一起切換。編輯或儲存後，它就是你的文件，切換語言會保留內容；開啟的檔案也會保留。

## 段落與換行

Enter 建立新段落，Shift+Enter 插入強制換行。長段落隨視窗寬度自動折行，不會修改 Markdown 原始碼。
這是同一段落中的原始碼軟換行。  
這一行之前是強制換行，原始碼在換行前保留兩個空格。

## 標題

上方文件標題是 H1，本節標題是 H2。在區塊開頭輸入一至六個 `#` 後輸入內文，即可建立標題；缺少的分隔空格會自動補齊。點選標題可編輯標記，游標離開後再次收起。

### H3 — 三級標題

#### H4 — 四級標題

##### H5 — 五級標題

###### H6 — 六級標題

Setext 一級標題
=================

Setext 二級標題
-----------------

Setext 用下一行的 `=` 表示 H1，用 `-` 表示 H2。點選標題可顯示並編輯底線。

## 行內格式、連結與跳脫

**粗體** · *斜體* · ~~刪除線~~ · __底線標記的粗體__ · _底線標記的斜體_ · **粗體中的 *巢狀斜體*** · [範例連結](https://example.org)。

點選行內格式或用方向鍵進入，會展開自身及必要的外層標記。輸入起始標記後，首個內文字元補齊有效的結束標記；再輸入相同的結束字元可以越過。連結、行內程式碼和單美元公式也支援補全。程式碼和原始碼模式按字面輸入。

行內程式碼：`**字面內容**` 和 ``程式碼中的 `反引號` ``。程式碼內部不解析 Markdown。使用反斜線跳脫標點：\*不是斜體\*，\[不是連結\]。

連結可以帶標題：[Ryne](https://example.org "Ryne 網站"). 自動連結：<https://example.org> 和 <mailto:hello@example.org>。

## 清單與任務

- 帶有 **粗體** 的無序項目。
  - 巢狀項目和公式：$a^2 + b^2 = c^2$。
  - 另一個子項目。
- 另一個頂層項目。  
  強制換行後仍屬於這個項目。

1. 有序項目。
2. 另一個有序項目。
   1. 巢狀有序項目。

- [ ] 尚未完成的任務。
- [x] 已經完成的任務。

無序清單也支援 `+` 和 `*` 標記：

+ 另一種無序標記。
* 還有一種。

有序清單可以從 1 以外的數字開始：

7. 從七開始編號。
8. 下一個項目。

非空項目按 Enter 接續，空項目按 Enter 退出一層。Tab / Shift+Tab 縮排或減少縮排目前項目及其子項；首個同層項目不能縮排。Shift+Enter 在項目內換行。點擊任務前的核取方塊切換完成狀態，支援復原與重做；也可透過區塊原始碼編輯任務標記。

懸停或聚焦項目時，前方三點選單可插入、移動、縮排、減少縮排或刪除項目及子項。最外層清單左上角的類型按鈕可選取或複製完整清單。Alt+Enter 開啟目前項目的選單。

## 程式碼區塊

支援反引號圍欄、波浪號圍欄和四空格縮排程式碼區塊。圍欄收起為帶語言標籤的程式碼區域。Enter 保留程式碼縮排並在區塊內換行。輸入起始圍欄及選用語言後按 Enter，一次補齊結束圍欄；可一次復原。透過區塊原始碼修改圍欄和語言。

```rust
fn main() {
    println!("Hello, Ryne!");
}
```

~~~python
def greet(name):
    return f"Hello, {name}!"
~~~

    **縮排程式碼中的字面 Markdown**
    [連結也保持字面內容](https://example.org)

- 清單中也可以巢狀放置程式碼：
  ```json
  {"editor": "Ryne", "ready": true}
  ```

語法醒目提示支援 Rust、Python、JavaScript/JSX、TypeScript/TSX、JSON 和 Bash，以及 `rs`、`py`、`js`、`ts`、`sh` 等別名。圍欄程式碼區塊左上角的語言選單可選常見語言和預設的 `auto`；選擇 `auto` 清空圍欄資訊欄位。自動辨識只影響醒目提示，不修改原始碼。手動切換可復原。選單中的其他語言及有歧義的程式碼維持一般樣式。

## Mermaid 圖表

```mermaid
flowchart LR
    A[開啟] --> B[編輯] --> C[儲存]
```

游標離開已閉合的 `mermaid` 區塊後顯示圖表。點選預覽或用方向鍵進入即可編輯同一份原始碼，Escape 再次收起。無效、不完整或超限的圖表保留可編輯程式碼與診斷。

## 行內公式

能量：$E = mc^2$。分數與下標：$\frac{a_1}{\sqrt{b}}$。

在單行 LaTeX 兩側各寫一個 `$`。點選或用方向鍵進入公式編輯原始碼，離開後顯示公式。清單和表格儲存格中也可使用。缺字及不支援的公式保留原始碼。

## 區塊公式

求和公式：

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$

多行矩陣：

$$
\begin{pmatrix}
1 & 2 \\
3 & 4
\end{pmatrix}
$$

在獨立段落中以 `$$` 包圍 LaTeX，離開後置中顯示；點選或以方向鍵進入可編輯完整原始碼。單獨輸入 `$$` 後按 Enter 補齊結束標記，內文中的 Enter 插入換行。支援清單、引用與 PDF 匯出；原始碼模式維持字面輸入。空白或未被解析器識別的公式、部分對齊環境保留原始碼。

## 表格

| 靠左對齊 | 置中對齊 | 靠右對齊 |
| :--- | :---: | ---: |
| **粗體儲存格** | `code` | 123 |
| 第一行<br>第二行 | 跳脫管線符號：a\|b | $x^2$ |
| 旁邊是空儲存格 | | 456 |

點選儲存格直接編輯。Tab / Shift+Tab 切換儲存格，Enter 移至下一列同欄；移過末列時可以新增一列。Shift+Enter 在儲存格內插入 `<br>`。輸入或貼上的管線符號會自動跳脫。

儲存格三點選單可調整欄對齊、插入、移動或刪除列欄。Alt+Enter 開啟目前儲存格選單，方向鍵、Enter 和 Escape 用於導覽。透過區塊原始碼編輯分隔符號。欄數不齊的表格保留原始碼；調整欄寬和跨儲存格 TSV 貼上尚未提供。

## 提示區塊（Alerts）

以下展示五種 Alerts 語法。類型標記保持英文，內文可以直接修改；不同提示區塊之間留一個空行。

> [!NOTE]
> 這裡是備註內容，用於強調使用者即使快速瀏覽也應該知道的資訊。

> [!TIP]
> 這裡是提示內容，提供實用的建議或技巧。

> [!IMPORTANT]
> 這裡是重要內容，說明完成目標所需的關鍵資訊。

> [!WARNING]
> 這裡是警告內容，提醒使用者立即注意，以避免發生問題。

> [!CAUTION]
> 這裡是注意事項，說明某些操作可能帶來的風險或負面後果。

## 引用、圖片、分隔線與原始文字

引用隱藏 `>`，正文前顯示引用圖示，文字區塊帶淺色背景；可直接編輯正文和行內格式，區塊原始碼入口可修改結構標記。巢狀引用保留縮排。非空引用中 Enter 自動接續，新行延續背景與縮排；空引用行 Enter 退出一層，Shift+Enter 插入引用內強制換行。區塊或全文原始碼中 Enter 保持原始換行。空引用草稿和原始文字保留可編輯原始碼。獨佔一段的 HTTP(S)、FTP、`file:` 和 `data:` 圖片可預覽，點擊後編輯 Markdown；離開後恢復圖片。支援 PNG、JPEG、GIF 首幀、WebP 和 SVG。Data URL 支援 Base64 和百分號編碼，本機檔案使用 `file:///...` 位址。載入失敗時顯示裂圖圖示和帶刪除線的紅色位址，超過 256 個字元時加上省略號；點擊可編輯完整 Markdown。與文字混排的圖片保留原始碼。

> 帶有 **行內粗體** 的引用。
>
> 同一個引用區塊中的第二個段落。
> > 巢狀引用。

![載入失敗](https://example.org/image.png)

![SVG 範例](data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHdpZHRoPSI2NDAiIGhlaWdodD0iMzYwIiB2aWV3Qm94PSIwIDAgNjQwIDM2MCI+CiAgPHJlY3QgeD0iMiIgeT0iMiIgd2lkdGg9IjYzNiIgaGVpZ2h0PSIzNTYiIHJ4PSIyNCIgZmlsbD0iI2YzZjZmYSIgc3Ryb2tlPSIjZDVkZGU3IiBzdHJva2Utd2lkdGg9IjQiLz4KICA8Y2lyY2xlIGN4PSI0NjIiIGN5PSIxMDYiIHI9IjM0IiBmaWxsPSIjZjRjNzZiIi8+CiAgPHBhdGggZD0iTTUyIDI4NCAyMTAgMTE2IDM2NiAyODRaIiBmaWxsPSIjOWJiN2NhIi8+CiAgPHBhdGggZD0ibTI3MCAyODQgMTI2LTEyNiAxOTIgMTI2WiIgZmlsbD0iIzYzOGJhNiIvPgogIDxwYXRoIGQ9Im0xNzIgMTU2IDM4LTQwIDM4IDQxLTM5LTEyWiIgZmlsbD0iI2ZmZiIvPgogIDxwYXRoIGQ9Ik01MiAyODRoNTM2IiBmaWxsPSJub25lIiBzdHJva2U9IiM0OTZiODEiIHN0cm9rZS13aWR0aD0iNCIgc3Ryb2tlLWxpbmVjYXA9InJvdW5kIi8+Cjwvc3ZnPgo=)

---

`***` 和 `___` 等其他分隔線寫法也能辨識。
***

<div>原始 HTML 保留可編輯原始碼。</div>

上方獨占一行的 `---` 是分隔線，與前後段落之間各留一個空行。不完整或尚未支援的語法保持可編輯，不丟棄內容。

## 原始碼、插入與檔案

巢狀內容共用最外層區塊左上角的類型按鈕。將指標移到按鈕上可開啟選單，編輯、選取或複製整個區塊的原始碼。Cmd+Shift+B 切換區塊原始碼，Cmd+Shift+M 切換全文原始碼，Escape 關閉選單或收起原始碼。原始碼模式中的輸入和換行均按字面處理。

區塊類型選單末尾可選擇「在下方新增」或「在上方新增」，再選擇區塊類型；文件第一個區塊僅提供下方。新增可一次復原，Escape 從類型選擇返回區塊選單。

懸停文件末尾下方會顯示 `+`，可插入上述各類區塊。未閉合結構需要先補全，才能新增區塊。

Cmd+O 開啟檔案，Cmd+S 儲存，Cmd+Shift+S 另存新檔。Cmd+Z / Cmd+Shift+Z 復原和重做。Cmd+A/C/X/V 全選、複製原始碼、剪下和貼上。選取和貼上可跨行、跨區塊。查看標記、選單或原始碼不會產生未儲存修改；儲存保留原始碼位元組和換行。輸入法確認文字作為一次可復原編輯提交。

點擊工具列的「排版」可整理全文 Markdown 原始碼，並可一次復原。規則會修復缺少空格的獨立標題與工作標記，保留程式碼內容、段落換行及容器縮排；儲存不會自動排版。
