# Assistant and updates

## Optional assistant

Open Settings → Assistant to configure the supported API key or subscription
token. Credentials are stored in the OS credential store. The assistant does not
run until configured and used. Open it with the sparkle icon in the titlebar.

Requests include your conversation and the selected database's schema and SQL
dialect. Including query results is a separate setting. Review generated SQL
before choosing Run or Insert. Database safe modes also apply to executed SQL.

Stop cancels the active stream. Closing the drawer, clearing the conversation,
or leaving the workspace also stops it. History, prompts, and stream buffers
have size limits; failures are shown in the conversation.

## Updates

Tables checks GitHub releases at launch and hourly when automatic checks are
enabled. Settings → Updates takes effect immediately. Use the titlebar download
icon or Tables → Check for Updates to check manually.

Installation requires confirmation. macOS upgrades verify the published checksum
and expected signing identity. Linux AppImages use the published checksum before
replacement. Other installation types open the release page; package-manager
users can update through their package manager. Windows builds are beta and
currently unsigned.

Automatic checks can be disabled independently of the assistant.
