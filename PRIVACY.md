# Privacy Policy for Notepad-matrix

**Effective Date:** November 22, 2026  
**Last Updated:** November 24, 2026

**Notepad-matrix** ("the App") is developed and maintained by Wei Ding.

## 1. Data Collection
The developer does not collect document contents, SSH credentials, usage analytics, telemetry, or crash logs through the App. The App does not include advertising or tracking SDKs and does not require an account with the developer.

Document editing and Markdown and Mermaid rendering run locally. The optional SSH terminal connects to a server you select. Opening an external link uses the system's configured application.

## 2. Local Document & Data Processing
* **Documents and Files:** Documents, preferences, sessions, backups, macros, and custom language configurations are stored on your Mac or in locations you select. Storage providers such as iCloud Drive handle files under their own privacy policies.
* **Markdown & Mermaid Preview:** All Markdown rendering and Mermaid diagram generation are performed locally within the App's sandboxed environment without sending your content to any third-party cloud service.
* **External Links:** Following a link opens the system's configured application. The destination service's privacy policy applies to that interaction.

## 3. SSH Connections and Credentials
* **Server Communication:** Connecting sends network traffic to the host you configure. The server and network services involved in the connection may receive your IP address and connection information. SSH encrypts authentication and terminal communication after key exchange.
* **Terminal Data:** The selected SSH server receives your username, authentication exchange, commands, pasted text, and other terminal input. Commands on the remote server can access or transmit data under that server's configuration. The developer does not receive this traffic through an intermediary service.
* **Passwords:** Entered SSH passwords are used for authentication. The App does not save them in its settings or credential files.
* **Keys and Server Identity:** If you generate an SSH key in the App, the private key is stored locally as an unencrypted OpenSSH key file with owner-only permissions. Public-key authentication does not send the private key to the server. Terminal profiles and approved server host keys are stored locally.

## 4. macOS App Sandbox & Permissions
The Mac App Store version runs inside the macOS App Sandbox. It stores application data in its container and accesses documents and directories you authorize through file dialogs or drag-and-drop. Security-scoped bookmarks can retain those grants for later access. Network access supports the SSH connections you request.

## 5. Data Retention & Deletion
Documents, settings, recovery backups, SSH profiles, generated keys, and approved host keys remain in their local storage until removed. You can delete documents and backups and remove stored application data on your Mac. Data retained by an SSH server or a storage provider is controlled by that service, not by the App.

## 6. Children's Privacy
Notepad-matrix does not knowingly collect any personal information from anyone, including children under the age of 13.

## 7. Changes to This Privacy Policy
We may update this Privacy Policy from time to time to reflect app updates or legal requirements. Any updates will be posted on this page with an updated revision date.

## 8. Contact Us
If you have any questions or concerns about this Privacy Policy or Notepad-matrix, please open an issue on our official support page:
* **Support & Issues:** [https://github.com/skyfiredao/notepad-matrix/issues](https://github.com/skyfiredao/notepad-matrix/issues)
