const TELEGRAM_LIMIT: usize = 4096;

pub fn split_for_telegram(text: &str) -> Vec<String> {
    if text.len() <= TELEGRAM_LIMIT {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= TELEGRAM_LIMIT {
            chunks.push(remaining.to_string());
            break;
        }

        let split_at = remaining[..TELEGRAM_LIMIT]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(TELEGRAM_LIMIT);

        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }

    chunks
}
