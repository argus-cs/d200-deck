/** Moves the item at `from` to `to`, in place. */
export function moveItem<T>(items: T[], from: number, to: number): void {
  const [item] = items.splice(from, 1);
  items.splice(to, 0, item);
}
