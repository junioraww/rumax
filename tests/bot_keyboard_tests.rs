use rumax::models::{
    BotInfo, InlineKeyboardAttachment, InlineKeyboardButton,
};

#[test]
fn test_inline_keyboard_serialization() {
    let button1 = InlineKeyboardButton::callback("Button 1", "payload_1");
    let button2 = InlineKeyboardButton::url("Link", "https://example.com");

    let keyboard = InlineKeyboardAttachment::new(
        Some("cb-123".into()),
        vec![vec![button1, button2]],
    );

    let json = serde_json::to_value(&keyboard).expect("serialize");
    assert_eq!(json["_type"], "INLINE_KEYBOARD");
    assert_eq!(json["callbackId"], "cb-123");
    assert_eq!(
        json["keyboard"]["buttons"][0][0]["type"],
        "CALLBACK"
    );
    assert_eq!(
        json["keyboard"]["buttons"][0][0]["text"],
        "Button 1"
    );
    assert_eq!(
        json["keyboard"]["buttons"][0][0]["payload"],
        "payload_1"
    );
    assert_eq!(
        json["keyboard"]["buttons"][0][1]["type"],
        "LINK"
    );
    assert_eq!(
        json["keyboard"]["buttons"][0][1]["url"],
        "https://example.com"
    );
}

#[test]
fn test_bot_info_parsing() {
    let raw = serde_json::json!({
        "commands": [
            {"name": "start", "description": "Start command"},
            {"name": "help", "description": "Help command"}
        ],
        "contact": {
            "id": 10001,
            "names": [{"name": "TestBot"}],
            "options": ["BOT"],
            "description": "Test bot description"
        }
    });

    let bot_info = BotInfo::from_payload(10001, &raw);
    assert_eq!(bot_info.bot_id, 10001);
    assert_eq!(bot_info.commands.len(), 2);
    assert_eq!(bot_info.commands[0].slash(), "/start");
    assert_eq!(
        bot_info.commands[0].description.as_deref(),
        Some("Start command")
    );
    assert_eq!(bot_info.commands[1].slash(), "/help");
    assert_eq!(
        bot_info.description.as_deref(),
        Some("Test bot description")
    );
}

#[test]
fn test_chat_folder_serialization() {
    let raw = serde_json::json!({
        "folders": [
            {
                "id": "synthetic-folder-id",
                "title": "Work",
                "include": [101, 102],
                "filters": [2, 3],
                "options": [0],
                "favorites": [101]
            }
        ],
        "foldersOrder": ["synthetic-folder-id"],
        "folderSync": 12345
    });

    let payload: rumax::models::FoldersPayload = serde_json::from_value(raw).expect("deserialize");
    assert_eq!(payload.folders.len(), 1);
    assert_eq!(payload.folders[0].id, "synthetic-folder-id");
    assert_eq!(payload.folders[0].title, "Work");
    assert_eq!(payload.folders[0].include, vec![101, 102]);
    assert_eq!(payload.folders[0].filters, vec![2, 3]);
    assert_eq!(payload.folders_order, vec!["synthetic-folder-id"]);
    assert_eq!(payload.folder_sync, Some(12345));
}
