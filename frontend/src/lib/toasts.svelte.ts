export type ToastKind = "success" | "error" | "info";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

let items = $state<ToastItem[]>([]);
let nextId = 0;

function push(kind: ToastKind, message: string, duration = 4000): void {
  const id = nextId++;
  items.push({ id, kind, message });
  if (duration > 0) {
    setTimeout(() => dismiss(id), duration);
  }
}

function dismiss(id: number): void {
  const index = items.findIndex((item) => item.id === id);
  if (index !== -1) items.splice(index, 1);
}

export const toast = {
  success: (message: string) => push("success", message),
  error: (message: string) => push("error", message),
  info: (message: string) => push("info", message),
  dismiss,
  get items() {
    return items;
  },
};
