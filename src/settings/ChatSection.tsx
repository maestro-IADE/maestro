import { BUBBLE_WIDTHS, CHAT_FONTS, CHAT_LEADINGS, CHAT_SIZES, CHAT_WIDTHS, useSettings } from "../settings";
import { Group, Row, Seg } from "./parts";

/** A Seg's options from one of the settings' step tables (id → { label }). */
const opts = <K extends string>(table: Record<K, { label: string }>) => (Object.keys(table) as K[]).map((id) => [id, table[id].label] as const);

/**
 * 채팅: what only the conversation with an agent has. Its font, size, line spacing and widths, with a preview.
 * (docs/features/ui/settings-dialog.md; the look shared by the whole app is 모양.)
 */
export function ChatSection() {
  const s = useSettings();
  return (
    <Group title="글과 폭">
      <Row label="글꼴" desc={(CHAT_FONTS[s.chatFont] ?? CHAT_FONTS.default).hint}>
        <Seg value={s.chatFont ?? "default"} options={opts(CHAT_FONTS)} onChange={(v) => s.set({ chatFont: v })} />
      </Row>
      <Row label="글자 크기" desc="전체 화면 기준이에요. 캔버스 블록은 한 단계 작아요.">
        <Seg value={s.chatSize ?? "md"} options={opts(CHAT_SIZES)} onChange={(v) => s.set({ chatSize: v })} />
      </Row>
      <Row label="줄 간격">
        <Seg value={s.chatLeading ?? "normal"} options={opts(CHAT_LEADINGS)} onChange={(v) => s.set({ chatLeading: v })} />
      </Row>
      <Row label="대화 폭" desc="전체 화면에서 대화가 놓이는 가운데 칸의 폭">
        <Seg value={s.chatWidth ?? "normal"} options={opts(CHAT_WIDTHS)} onChange={(v) => s.set({ chatWidth: v })} />
      </Row>
      <Row label="말풍선 폭" desc="내가 보낸 메시지 말풍선이 넓어질 수 있는 만큼">
        <Seg value={s.chatBubble ?? "normal"} options={opts(BUBBLE_WIDTHS)} onChange={(v) => s.set({ chatBubble: v })} />
      </Row>
      <div className="chat-preview" aria-label="채팅 미리보기">
        <div className="msg-user"><div className="bubble">로그인 화면에서 비밀번호를 틀리면 어떻게 돼?</div></div>
        <div className="msg-text">세 번까지는 다시 입력할 수 있고, 네 번째부터는 5분 동안 잠겨요. 잠긴 동안에는 화면에 남은 시간이 보여요.</div>
      </div>
    </Group>
  );
}
