import { beforeEach, describe, expect, it } from "vitest";
import { useToastStore } from "./toastStore";

describe("useToastStore", () => {
  beforeEach(() => {
    useToastStore.getState().clear();
  });

  it("push returns increasing ids", () => {
    const a = useToastStore.getState().push("one");
    const b = useToastStore.getState().push("two");
    expect(b).toBeGreaterThan(a);
    expect(useToastStore.getState().toasts.map((t) => t.id)).toEqual([a, b]);
  });

  it("dismiss removes by id", () => {
    const a = useToastStore.getState().push("keep");
    const b = useToastStore.getState().push("drop");
    useToastStore.getState().dismiss(b);
    expect(useToastStore.getState().toasts.map((t) => t.id)).toEqual([a]);
  });

  it("caps the list at 4 by dropping the oldest", () => {
    const ids = [1, 2, 3, 4, 5].map((n) => useToastStore.getState().push(`m${n}`));
    const remaining = useToastStore.getState().toasts.map((t) => t.id);
    expect(remaining).toHaveLength(4);
    expect(remaining).toEqual(ids.slice(1));
  });
});
