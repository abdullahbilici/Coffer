import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { Run } from "../app/actions";
import {
  api,
  type AssistantEvent,
  type Conversation,
  type Message,
  type ReplyPhase,
} from "../lib/api";
import type { Session } from "./useSession";

export type Assistant = ReturnType<typeof useAssistant>;

/** Mirrors `commands::assistant::ASSISTANT_EVENT`. */
const ASSISTANT_EVENT = "assistant-event";

/** The open conversation. `id` null is a new chat that has no messages yet. */
export type Thread = {
  id: number | null;
  messages: Message[];
  /** Reply text streaming in, not stored yet. */
  streaming: string;
  /** Where the model call in progress is; `null` between calls and once the reply ends. */
  phase: ReplyPhase | null;
  /** Why the last reply failed. */
  error: string | null;
  /** Changes whenever another conversation is opened, not when a new chat gets its id. */
  opened: number;
};

const NEW_THREAD: Thread = {
  id: null,
  messages: [],
  streaming: "",
  phase: null,
  error: null,
  opened: 0,
};

/**
 * Starts loading the model if the server has unloaded it, so the wait happens while the user
 * types. Fire and forget: the next reply waits for the load anyway.
 */
export const warmUp = () => void api.warmUpAssistant().catch(() => {});

const byActivity = (a: Conversation, b: Conversation) => b.updatedAt - a.updatedAt || b.id - a.id;

/** The list with one conversation changed, most recently active first. */
const updated = (
  list: Conversation[] | null,
  id: number,
  change: (c: Conversation) => Conversation,
) => list && list.map((c) => (c.id === id ? change(c) : c)).sort(byActivity);

/**
 * Conversations with the assistant. Lives in `App` so a reply keeps streaming while the user is
 * elsewhere. History is decrypted, so it loads inside the unlock window and is dropped as soon
 * as the window closes, like revealed secrets.
 */
export function useAssistant(session: Session, run: Run, reload: () => Promise<void>) {
  /** `null` until loaded, and again once the unlock window closes. */
  const [conversations, setConversations] = useState<Conversation[] | null>(null);
  const [thread, setThread] = useState<Thread>(NEW_THREAD);
  const threadId = useRef<number | null>(null);
  threadId.current = thread.id;
  const listed = useRef(conversations);
  listed.current = conversations;

  const forget = useCallback(() => {
    setConversations(null);
    setThread(NEW_THREAD);
  }, []);

  useEffect(() => {
    if (!session.unlocked) forget();
  }, [session.unlocked, forget]);

  // Streamed text arrives per token; the latest is applied once per frame so replies stay smooth.
  const pending = useRef<{ id: number; text: string } | null>(null);
  const frame = useRef(0);
  const flush = useCallback(() => {
    frame.current = 0;
    const chunk = pending.current;
    pending.current = null;
    if (!chunk) return;
    setThread((t) => (t.id === chunk.id ? { ...t, streaming: chunk.text } : t));
  }, []);

  useEffect(() => {
    const onEvent = (event: AssistantEvent) => {
      const id = event.conversationId;
      if (event.type === "text") {
        if (id !== threadId.current) return;
        pending.current = { id, text: event.text };
        if (!frame.current) frame.current = requestAnimationFrame(flush);
        return;
      }
      if (event.type === "phase") {
        const { phase } = event;
        setThread((t) => (t.id === id ? { ...t, phase } : t));
        return;
      }
      // Anything else ends the streamed text: it was stored, or the reply ended.
      cancelAnimationFrame(frame.current);
      frame.current = 0;
      pending.current = null;
      if (event.type === "message") {
        const { message } = event;
        setConversations((list) =>
          updated(list, id, (c) => ({ ...c, updatedAt: message.createdAt })),
        );
        setThread((t) =>
          t.id === id
            ? { ...t, messages: [...t.messages, message], streaming: "", phase: null }
            : t,
        );
      } else {
        // Quick capture starts conversations of its own; a loaded list picks them up.
        if (listed.current && !listed.current.some((c) => c.id === id)) {
          api.listConversations().then(
            (list) => setConversations(list.sort(byActivity)),
            () => {},
          );
        }
        setConversations((list) => updated(list, id, (c) => ({ ...c, replying: false })));
        const error = event.type === "failed" ? event.message : null;
        setThread((t) => (t.id === id ? { ...t, streaming: "", phase: null, error } : t));
      }
    };
    const unlisten = listen<AssistantEvent>(ASSISTANT_EVENT, (e) => onEvent(e.payload));
    return () => {
      cancelAnimationFrame(frame.current);
      void unlisten.then((stop) => stop());
    };
  }, [flush]);

  /** Loads the list, asking for Touch ID if the window is closed. */
  const load = useCallback(
    () =>
      run(async () => {
        const list = await session.withUnlock(api.listConversations);
        setConversations(list.sort(byActivity));
      }),
    [run, session],
  );

  const open = useCallback(
    (id: number | null) =>
      run(async () => {
        if (id === null) {
          // At once, so a `send` right after this starts a new conversation.
          threadId.current = null;
          setThread({ ...NEW_THREAD, opened: Date.now() });
          return;
        }
        const messages = await session.withUnlock(() => api.getConversation(id));
        setThread({ ...NEW_THREAD, id, messages, opened: Date.now() });
      }),
    [run, session],
  );

  const send = useCallback(
    (text: string, paste: string | null = null) =>
      run(async () => {
        const sent = await session.withUnlock(() => api.sendMessage(threadId.current, text, paste));
        const { conversation, message } = sent;
        setConversations((list) =>
          [conversation, ...(list ?? []).filter((c) => c.id !== conversation.id)].sort(byActivity),
        );
        setThread((t) =>
          t.id === conversation.id
            ? { ...t, messages: [...t.messages, message], error: null }
            : { ...NEW_THREAD, id: conversation.id, messages: [message], opened: t.opened },
        );
      }),
    [run, session],
  );

  /** Puts a stored message back in the open thread, e.g. a draft that was just applied. */
  const replace = useCallback((id: number, message: Message) => {
    setThread((t) =>
      t.id === id
        ? { ...t, messages: t.messages.map((m) => (m.id === message.id ? message : m)) }
        : t,
    );
  }, []);

  /** Applies a draft (Touch ID if the window is closed), then reloads the vault. */
  const applyDraft = useCallback(
    (messageId: number) => {
      const id = threadId.current;
      if (id === null) return Promise.resolve();
      return run(async () => {
        const message = await session.withUnlock(() => api.applyDraft(id, messageId));
        replace(id, message);
        await reload();
      });
    },
    [run, session, replace, reload],
  );

  const discardDraft = useCallback(
    (messageId: number) => {
      const id = threadId.current;
      if (id === null) return Promise.resolve();
      return run(async () => {
        replace(id, await session.withUnlock(() => api.discardDraft(id, messageId)));
      });
    },
    [run, session, replace],
  );

  const cancel = useCallback(() => {
    const id = threadId.current;
    if (id !== null) void run(() => api.cancelReply(id));
  }, [run]);

  const rename = useCallback(
    (id: number, title: string) =>
      run(async () => {
        await session.withUnlock(() => api.renameConversation(id, title));
        setConversations((list) => updated(list, id, (c) => ({ ...c, title: title.trim() })));
      }),
    [run, session],
  );

  const remove = useCallback(
    (id: number) =>
      run(async () => {
        await session.withUnlock(() => api.deleteConversation(id));
        setConversations((list) => list && list.filter((c) => c.id !== id));
        setThread((t) => (t.id === id ? NEW_THREAD : t));
      }),
    [run, session],
  );

  const replying = conversations?.find((c) => c.id === thread.id)?.replying ?? false;

  return useMemo(
    () => ({
      conversations,
      thread,
      replying,
      load,
      open,
      send,
      cancel,
      rename,
      remove,
      applyDraft,
      discardDraft,
    }),
    [
      conversations,
      thread,
      replying,
      load,
      open,
      send,
      cancel,
      rename,
      remove,
      applyDraft,
      discardDraft,
    ],
  );
}
