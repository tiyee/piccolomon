# Ryne Privacy Policy

Last updated: October 25, 2026

[简体中文](PRIVACY.zh-CN.md)

This Privacy Policy explains how the Ryne (如言) Markdown desktop application handles information. It applies to the Windows version distributed through the Microsoft Store.

## 1. Local document processing

Ryne is a local file-based Markdown editor. No Ryne account or sign-in is required.

When you open, edit, save, or export a document, Ryne accesses and processes the relevant file contents and paths on your device to provide editing, formatted display, undo and redo, saving, and PDF export. Markdown parsing, syntax highlighting, mathematical formula and Mermaid diagram rendering, and PDF generation all take place locally.

Ryne does not upload your document contents, file names, file paths, or editing history to developer-operated servers or send this information to online rendering or artificial intelligence services.

## 2. Information stored locally

To provide its basic functions, Ryne processes or stores the following information locally:

- **Documents and exported files:** Saved to locations you choose and managed by you.
- **Recent files:** Up to ten paths of files successfully opened or saved, so you can reopen them quickly. Paths may contain file names, user names, or folder names.
- **Preferences:** Your selected interface language and mathematical font. When following the system language, Ryne reads the system language setting.
- **Temporary editing data:** The current document, selection, undo and redo history, and rendering caches are held in memory to support the current editing session.
- **Temporary save files:** When saving a document or export, Ryne first writes a temporary file in the destination directory and then replaces the destination file. Temporary files are not retained separately after normal completion; an unexpected interruption may leave temporary files behind.

In a conventional Windows installation, preferences and recent file records are usually located in `%APPDATA%\Ryne`, in files named `language`, `math-font`, and `recent.json`. Windows application data redirection may affect the actual location for Microsoft Store installations.

## 3. Network connections and update checks

Routine document editing, formatted display, saving, and PDF export do not require an internet connection. Ryne does not automatically check for updates at startup.

When you choose “Check for Updates,” Ryne requests information about the latest stable Ryne release from GitHub over HTTPS. GitHub receives the IP address needed for the connection and network information such as the request time, requested address, and request headers. The application identifies itself as `Ryne` in the request. The request does not include your documents, file paths, or a unique device identifier. The installed version is compared with the latest version locally.

The update check does not automatically download or install the application. If you choose to view a release, the system opens a GitHub page in your browser. That visit is subject to your browser settings and the [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement). GitHub may process network request information outside your country or region.

The Microsoft Store may independently provide application distribution, updates, diagnostics, or usage statistics. These platform activities are governed by your system and Store settings and the [Microsoft Privacy Statement](https://www.microsoft.com/en-us/privacy/privacystatement).

## 4. Analytics, advertising, and sharing

The Ryne application does not integrate advertising, behavioral tracking, user profiling, usage analytics, or automatic crash report uploads. It does not read advertising identifiers or sell or rent your documents or personal information.

When you copy, cut, or paste, Ryne accesses the system clipboard according to your actions. Clipboard history and cross-device clipboard synchronization are controlled by Windows and related software.

If you save documents in a synchronized folder such as OneDrive, on a network drive, or in another third-party-managed location, the relevant service may process those files according to your chosen storage location and service settings. If you voluntarily send feedback, screenshots, logs, or sample files to the developer, the developer receives the information you provide to address the issue. Please avoid including unrelated personal information or confidential content.

## 5. Retention, access, and deletion

You can directly view, copy, move, or delete your Markdown documents and PDF files. Ryne does not provide a cloud account for storing these documents.

Recent file records remain until you clear them or subsequent file visits replace them. You can remove these records using the recent files clearing command in the File menu; clearing the list does not delete documents. Preferences remain until you change or delete the corresponding configuration.

To remove local application configuration, close Ryne and delete its configuration files, or use Windows application data management features where available. Uninstalling the application may not delete your saved documents, exported files, or all configuration. Backups and third-party synchronized copies must also be managed through the relevant services.

You can avoid GitHub requests initiated by the update checking feature by not using “Check for Updates.” Connections independently initiated by the Microsoft Store or operating system are managed through their own settings.

## 6. Data security

Ryne uses operating system file access mechanisms and saves files through same-directory temporary files and replacement operations to reduce the risk of file corruption from interrupted saves. Update checks use HTTPS.

Ryne does not provide additional application-level encryption for Markdown documents, PDFs, or preferences. You can protect local data through operating system account permissions, disk encryption, and backup tools.

## 7. Changes to this policy

This policy may be updated as application features or information handling practices change. Updates will be reflected in the “Last updated” date on this page. If data collection or network features are added in the future, the policy will be updated accordingly, with notice or consent where required by applicable law.
