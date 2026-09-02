export type CaptionMentionToken =
  | { kind: "text"; text: string }
  | { kind: "mention"; text: string; username: string };

const USERNAME_CHARACTER = /^[A-Za-z0-9_.]$/;
const MENTION_PREFIX_BLOCKER = /^[A-Za-z0-9_.@]$/;
const MAX_USERNAME_LENGTH = 30;

export function tokenizeCaptionMentions(
  caption: string,
): CaptionMentionToken[] {
  const tokens: CaptionMentionToken[] = [];
  let textStart = 0;
  let index = 0;

  while (index < caption.length) {
    if (caption[index] !== "@") {
      index += 1;
      continue;
    }

    const previousCharacter = index > 0 ? caption[index - 1] : undefined;
    if (
      previousCharacter !== undefined &&
      MENTION_PREFIX_BLOCKER.test(previousCharacter)
    ) {
      index += 1;
      continue;
    }

    let candidateEnd = index + 1;
    while (
      candidateEnd < caption.length &&
      USERNAME_CHARACTER.test(caption[candidateEnd])
    ) {
      candidateEnd += 1;
    }

    let mentionEnd = candidateEnd;
    while (mentionEnd > index + 1 && caption[mentionEnd - 1] === ".") {
      mentionEnd -= 1;
    }

    const username = caption.slice(index + 1, mentionEnd);
    if (username.length === 0 || username.length > MAX_USERNAME_LENGTH) {
      index = candidateEnd;
      continue;
    }

    if (textStart < index) {
      tokens.push({ kind: "text", text: caption.slice(textStart, index) });
    }
    tokens.push({
      kind: "mention",
      text: caption.slice(index, mentionEnd),
      username,
    });

    textStart = mentionEnd;
    index = candidateEnd;
  }

  if (textStart < caption.length) {
    tokens.push({ kind: "text", text: caption.slice(textStart) });
  }

  return tokens;
}
