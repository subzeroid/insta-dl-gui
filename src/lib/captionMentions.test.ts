import { describe, expect, it } from "vitest";

import { tokenizeCaptionMentions } from "./captionMentions";

describe("tokenizeCaptionMentions", () => {
  it("tokenizes mentions while preserving surrounding punctuation as text", () => {
    expect(tokenizeCaptionMentions("Play with @nike, then (@adidas).")).toEqual(
      [
        { kind: "text", text: "Play with " },
        { kind: "mention", text: "@nike", username: "nike" },
        { kind: "text", text: ", then (" },
        { kind: "mention", text: "@adidas", username: "adidas" },
        { kind: "text", text: ")." },
      ],
    );
  });

  it("keeps email, embedded, and repeated-at-sign forms as plain text", () => {
    const caption = "mail@example.com foo@nike and @@adidas";

    expect(tokenizeCaptionMentions(caption)).toEqual([
      { kind: "text", text: caption },
    ]);
  });

  it("keeps a bare at sign and an overlong username as plain text", () => {
    const caption = `@ and @${"a".repeat(31)}`;

    expect(tokenizeCaptionMentions(caption)).toEqual([
      { kind: "text", text: caption },
    ]);
  });

  it("accepts an exactly 30-character username before trailing punctuation", () => {
    const username = "a".repeat(30);

    expect(tokenizeCaptionMentions(`@${username}.`)).toEqual([
      { kind: "mention", text: `@${username}`, username },
      { kind: "text", text: "." },
    ]);
  });

  it("rejects mentions preceded by an ASCII digit, underscore, or period", () => {
    const caption = "1@nike _@nike end.@nike";

    expect(tokenizeCaptionMentions(caption)).toEqual([
      { kind: "text", text: caption },
    ]);
  });

  it("accepts a mention preceded by a non-ASCII character", () => {
    expect(tokenizeCaptionMentions("🔥@nike")).toEqual([
      { kind: "text", text: "🔥" },
      { kind: "mention", text: "@nike", username: "nike" },
    ]);
  });

  it("accepts underscores, digits, and internal periods in usernames", () => {
    expect(tokenizeCaptionMentions("@nike_2024.official")).toEqual([
      {
        kind: "mention",
        text: "@nike_2024.official",
        username: "nike_2024.official",
      },
    ]);
  });

  it("preserves uppercase letters in mention text and usernames", () => {
    expect(tokenizeCaptionMentions("@Nike_Official")).toEqual([
      {
        kind: "mention",
        text: "@Nike_Official",
        username: "Nike_Official",
      },
    ]);
  });

  it("continues scanning for valid mentions after invalid candidates", () => {
    const overlongMention = `@${"a".repeat(31)}`;
    const caption = `@ @nike and ${overlongMention} @adidas`;

    expect(tokenizeCaptionMentions(caption)).toEqual([
      { kind: "text", text: "@ " },
      { kind: "mention", text: "@nike", username: "nike" },
      { kind: "text", text: ` and ${overlongMention} ` },
      { kind: "mention", text: "@adidas", username: "adidas" },
    ]);
  });

  it("does not start another mention after a trailing period", () => {
    expect(tokenizeCaptionMentions("@nike.@adidas")).toEqual([
      { kind: "mention", text: "@nike", username: "nike" },
      { kind: "text", text: ".@adidas" },
    ]);
  });

  it("does not start another mention immediately after a username", () => {
    expect(tokenizeCaptionMentions("@nike@adidas")).toEqual([
      { kind: "mention", text: "@nike", username: "nike" },
      { kind: "text", text: "@adidas" },
    ]);
  });

  it("keeps a username starting with a non-ASCII letter as text", () => {
    expect(tokenizeCaptionMentions("@ñike")).toEqual([
      { kind: "text", text: "@ñike" },
    ]);
  });

  it("ends a mention before a non-ASCII letter", () => {
    expect(tokenizeCaptionMentions("@nikeé")).toEqual([
      { kind: "mention", text: "@nike", username: "nike" },
      { kind: "text", text: "é" },
    ]);
  });

  it("blocks ASCII letter prefixes but accepts non-ASCII letter prefixes", () => {
    expect(tokenizeCaptionMentions("X@nike é@adidas")).toEqual([
      { kind: "text", text: "X@nike é" },
      { kind: "mention", text: "@adidas", username: "adidas" },
    ]);
  });

  it("keeps an overlong username and trailing period as one text token", () => {
    const caption = `@${"a".repeat(31)}.`;

    expect(tokenizeCaptionMentions(caption)).toEqual([
      { kind: "text", text: caption },
    ]);
  });

  it("keeps an at sign without a username as one text token", () => {
    expect(tokenizeCaptionMentions("@#tag")).toEqual([
      { kind: "text", text: "@#tag" },
    ]);
  });

  it("preserves a newline in the text token before a mention", () => {
    expect(tokenizeCaptionMentions("first line\n@nike")).toEqual([
      { kind: "text", text: "first line\n" },
      { kind: "mention", text: "@nike", username: "nike" },
    ]);
  });

  it("keeps an at sign followed only by periods as one text token", () => {
    expect(tokenizeCaptionMentions("@...")).toEqual([
      { kind: "text", text: "@..." },
    ]);
  });

  it("creates independent tokens for repeated identical mentions", () => {
    expect(tokenizeCaptionMentions("@nike and @nike")).toEqual([
      { kind: "mention", text: "@nike", username: "nike" },
      { kind: "text", text: " and " },
      { kind: "mention", text: "@nike", username: "nike" },
    ]);
  });

  it("treats periods inside usernames as valid and trailing periods as text", () => {
    expect(tokenizeCaptionMentions("@team.nike...")).toEqual([
      { kind: "mention", text: "@team.nike", username: "team.nike" },
      { kind: "text", text: "..." },
    ]);
  });

  it.each(["", "plain caption", "first line\nsecond line"])(
    "preserves text-only input %j",
    (caption) => {
      expect(tokenizeCaptionMentions(caption)).toEqual(
        caption === "" ? [] : [{ kind: "text", text: caption }],
      );
    },
  );

  it.each([
    "Play with @nike, then (@adidas).",
    "mail@example.com foo@nike and @@adidas",
    `@ and @${"a".repeat(31)}`,
    "@nike and @nike",
    `@ @nike and @${"a".repeat(31)} @adidas`,
    "first line\n@team.nike...\nlast line",
  ])("reconstructs the exact caption for %j", (caption) => {
    const reconstructed = tokenizeCaptionMentions(caption)
      .map((token) => token.text)
      .join("");

    expect(reconstructed).toBe(caption);
  });
});
