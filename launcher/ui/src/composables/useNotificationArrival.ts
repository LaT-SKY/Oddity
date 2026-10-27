import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  watch,
  type Ref,
} from "vue";
import { createTimeline, svg, utils } from "animejs";
import {
  notifications,
  pauseNotifications,
  type NotificationItem,
} from "../core/notifications";

function pathOf(d: string) {
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", d);
  return path;
}
const inverseInOutQuad = (value: number) =>
  value < 0.5 ? Math.sqrt(value / 2) : 1 - Math.sqrt((1 - value) / 2);

export function useNotificationArrival(
  root: Ref<HTMLElement | null>,
  reduced: Ref<boolean>,
) {
  const displayed = ref<NotificationItem[]>([]);
  const landed = ref<number[]>([]);
  const incoming = ref<number | null>(null);
  const revealed = ref<number | null>(null);
  const retiring = ref<number[]>([]);
  const phase = ref(0);
  const known = new Set<number>();
  const exits = new Map<number, ReturnType<typeof createTimeline>>();
  let queue: number[] = [];
  let timeline: ReturnType<typeof createTimeline> | undefined;
  let run = 0;
  let mounted = false;
  const settled = computed(() =>
    displayed.value.filter((item) => landed.value.includes(item.id)),
  );

  function elements(id: number) {
    const slot = root.value!.querySelector<HTMLElement>(
      `[data-notification-id="${id}"]`,
    )!;
    return {
      slot,
      flight: slot.querySelector<HTMLElement>(".notification-flight")!,
      shell: slot.querySelector<HTMLElement>(".notification-surface")!,
    };
  }
  function normalize(flight: HTMLElement, shell: HTMLElement) {
    utils.set(flight, { x: 0, y: 0 });
    utils.set(shell, {
      width: "100%",
      height: "auto",
      transform: "none",
      borderRadius: 20,
      opacity: 1,
      visibility: "visible",
    });
  }
  function cancelEntry() {
    run++;
    timeline?.cancel();
    incoming.value = null;
    revealed.value = null;
    phase.value = 0;
  }
  function reorder() {
    if (incoming.value !== null || retiring.value.length) return;
    const order = notifications.map((item) => item.id);
    displayed.value.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
  }
  function drop(id: number) {
    displayed.value = displayed.value.filter((item) => item.id !== id);
    landed.value = landed.value.filter((candidate) => candidate !== id);
    retiring.value = retiring.value.filter((candidate) => candidate !== id);
    exits.delete(id);
    if (!retiring.value.length) pauseNotifications("departure", false);
    reorder();
  }
  async function retire(id: number) {
    if (displayed.value.find((item) => item.id === id)?.swiped) {
      drop(id);
      return;
    }
    retiring.value = [...retiring.value, id];
    pauseNotifications("departure", true);
    await nextTick();
    if (!mounted || !displayed.value.some((item) => item.id === id)) return;
    const { slot, flight, shell } = elements(id);
    if (reduced.value || getComputedStyle(shell).visibility === "hidden") {
      drop(id);
      return;
    }
    const base = slot
      .querySelector<HTMLElement>(".notification-depth")!
      .getBoundingClientRect();
    const scale = base.width / slot.offsetWidth;
    const box = shell.getBoundingClientRect();
    const logo = document
      .querySelector<HTMLElement>(".brand-mark")!
      .getBoundingClientRect();
    const startX = (box.left + box.width / 2 - base.left) / scale;
    const startY = (box.top + box.height / 2 - base.top) / scale;
    const endX = (logo.left + logo.width / 2 - base.left) / scale;
    const endY = (logo.bottom + 12 - base.top) / scale;
    utils.set(flight, { x: startX, y: startY });
    utils.set(shell, {
      width: `${box.width / scale}px`,
      height: `${box.height / scale}px`,
      transform: "translate(-50%, -50%)",
      visibility: "visible",
    });
    const path = pathOf(
      `M ${startX} ${startY} C ${startX - 32} ${startY + 24}, ${endX - 20} ${endY + 28}, ${endX} ${endY}`,
    );
    const travel = svg.createMotionPath(path);
    const exit = createTimeline({
      onComplete: () => {
        if (mounted) drop(id);
      },
    })
      .add(
        flight,
        {
          translateX: travel.translateX,
          translateY: travel.translateY,
          duration: 260,
          ease: "inOut(2)",
        },
        0,
      )
      .add(
        shell,
        {
          width: [
            { to: "40px", duration: 150 },
            { to: "14px", duration: 110 },
          ],
          height: [
            { to: "40px", duration: 150 },
            { to: "14px", duration: 110 },
          ],
          borderRadius: [
            { to: "14px", duration: 150 },
            { to: 7, duration: 110 },
          ],
          ease: "inOut(2)",
        },
        0,
      )
      .add(shell, { opacity: 0, duration: 90, ease: "out(2)" }, 170);
    exits.set(id, exit);
  }
  async function pump() {
    if (!mounted || incoming.value !== null) return;
    const candidate = queue.shift();
    if (candidate === undefined) {
      pauseNotifications("arrival", false);
      reorder();
      return;
    }
    const id: number = candidate;
    const item = notifications.find((item) => item.id === id);
    if (!item) {
      void pump();
      return;
    }
    pauseNotifications("arrival", true);
    const current = ++run;
    incoming.value = id;
    revealed.value = null;
    phase.value = 1;
    displayed.value = [item, ...displayed.value];
    await nextTick();
    if (!mounted || run !== current) return;
    const { slot, flight, shell } = elements(id);
    const card = shell.querySelector<HTMLElement>(".notification-card")!;
    const width = card.offsetWidth + 2;
    const height = card.offsetHeight + 2;
    const logo = document
      .querySelector<HTMLElement>(".brand-mark")!
      .getBoundingClientRect();
    const origin = slot.getBoundingClientRect();
    const startX = logo.left + logo.width / 2 - origin.left;
    const startY = logo.bottom + 12 - origin.top;
    const bendX = Math.max(24 - origin.left, startX - 32);
    const bendY = startY + 28;
    const squareSize = Math.min(
      52,
      Math.max(32, logo.left + logo.width / 2 - 4),
    );
    const squareX = Math.max(squareSize / 2 + 4 - origin.left, startX - 24);
    const squareY = Math.max(height / 2 + 16, bendY + 18);
    const leg = `M ${startX} ${startY} C ${startX - 22} ${startY + 2}, ${bendX - 8} ${bendY - 12}, ${bendX} ${bendY} C ${bendX + 8} ${bendY + 12}, ${squareX - 18} ${squareY + 12}, ${squareX} ${squareY}`;
    const path = pathOf(
      `${leg} C ${squareX + 36} ${squareY - 24}, ${width / 2 - 48} ${height / 2 - 12}, ${width / 2} ${height / 2}`,
    );
    const duration = 420;
    const turn = Math.round(
      duration *
        inverseInOutQuad(pathOf(leg).getTotalLength() / path.getTotalLength()),
    );
    const growAt = Math.min(75, turn * 0.4);
    const travel = svg.createMotionPath(path);
    function land() {
      if (run !== current) return;
      phase.value = 3;
      landed.value = [...landed.value, id];
    }
    function finish() {
      if (run !== current) return;
      normalize(flight, shell);
      incoming.value = null;
      revealed.value = null;
      phase.value = 0;
      void pump();
    }
    // 位移交给一条连续的中心轨迹, 形变与内容显露独立编排。
    utils.set(flight, { x: startX, y: startY });
    utils.set(shell, {
      width: "14px",
      height: "14px",
      transform: "translate(-50%, -50%)",
      borderRadius: 7,
      opacity: 0,
      visibility: "visible",
    });
    timeline = createTimeline({ onComplete: finish })
      .add(
        flight,
        {
          translateX: travel.translateX,
          translateY: travel.translateY,
          duration,
          ease: "inOut(2)",
        },
        0,
      )
      .add(shell, { opacity: 1, duration: 45, ease: "out(2)" }, 0)
      .call(() => {
        if (run === current) phase.value = 2;
      }, 22)
      .add(
        shell,
        {
          width: [
            { to: "18px", duration: growAt, ease: "linear" },
            {
              to: `${squareSize}px`,
              duration: turn - growAt,
              ease: "inOut(2)",
            },
            { to: `${width}px`, duration: duration - turn, ease: "inOut(2)" },
          ],
          height: [
            { to: "18px", duration: growAt, ease: "linear" },
            {
              to: `${squareSize}px`,
              duration: turn - growAt,
              ease: "inOut(2)",
            },
            { to: `${height}px`, duration: duration - turn, ease: "inOut(2)" },
          ],
          borderRadius: [
            { to: 16, duration: turn, ease: "inOut(2)" },
            { to: 20, duration: duration - turn, ease: "inOut(2)" },
          ],
        },
        0,
      )
      .call(land, turn)
      .call(() => {
        if (run === current) revealed.value = id;
      }, turn + 55);
  }
  function reconcile() {
    const ids = notifications.map((item) => item.id);
    displayed.value = displayed.value.map(
      (item) => notifications.find((live) => live.id === item.id) ?? item,
    );
    queue = queue.filter((id) => ids.includes(id));
    if (incoming.value !== null && !ids.includes(incoming.value)) cancelEntry();
    for (const item of displayed.value)
      if (!ids.includes(item.id) && !retiring.value.includes(item.id))
        void retire(item.id);
    for (const id of [...ids].reverse())
      if (!known.has(id)) {
        known.add(id);
        queue.push(id);
      }
    if (reduced.value) {
      cancelEntry();
      queue = [];
      exits.forEach((exit) => exit.cancel());
      exits.clear();
      retiring.value = [];
      displayed.value = [...notifications];
      landed.value = ids;
      root.value
        ?.querySelectorAll<HTMLElement>(".notification-slot")
        .forEach((slot) => {
          normalize(
            slot.querySelector<HTMLElement>(".notification-flight")!,
            slot.querySelector<HTMLElement>(".notification-surface")!,
          );
        });
      pauseNotifications("arrival", false);
      pauseNotifications("departure", false);
    } else void pump();
  }
  watch(
    () => notifications.map((item) => `${item.id}:${item.revision}`).join(","),
    reconcile,
    { flush: "post" },
  );
  watch(reduced, reconcile, { flush: "post" });
  onMounted(() => {
    mounted = true;
    reconcile();
  });
  onBeforeUnmount(() => {
    mounted = false;
    cancelEntry();
    exits.forEach((exit) => exit.cancel());
    pauseNotifications("arrival", false);
    pauseNotifications("departure", false);
  });
  return { displayed, landed, incoming, revealed, retiring, phase, settled };
}
