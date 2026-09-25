/**
 * SOURCE OF TRUTH KEYWORDS: jsdom layout stubs, stubListLayout, offsetHeight stub, scrollTop stub, scrollTo stub, virtualized list tests
 * WHAT:  `stubListLayout({ viewport, row })` gives jsdom just enough layout for a virtualized DataList: the element
 *        marked `data-slot="data-list-viewport"` is `viewport` px tall, every other element `row` px; its scroll
 *        height is the virtual list's height; scrollTop is kept and `scrollTo` scrolls and fires `scroll`. Returns
 *        the function that puts HTMLElement.prototype back.
 * WHY:   jsdom has no layout (every size and scroll position is 0), so TanStack Virtual would render nothing and
 *        never scroll. Stubbing the prototype for one test at a time keeps every other test on plain jsdom.
 * WHERE: components/global/data-list/DataList.test.tsx, routes/history/HistoryPage.test.tsx.
 */

const STUBBED = ["offsetHeight", "offsetWidth", "clientHeight", "scrollHeight", "scrollTop", "scrollTo"] as const;

export interface ListLayout {
  /** Height of the list's scrolling viewport, in px. */
  readonly viewport: number;
  /** Height of every other element (each row), in px. */
  readonly row: number;
}

export function stubListLayout({ viewport, row }: ListLayout): () => void {
  const proto = HTMLElement.prototype;
  const saved = STUBBED.map((name) => [name, Object.getOwnPropertyDescriptor(proto, name)] as const);
  const scrollTops = new WeakMap<HTMLElement, number>();
  const height = (element: HTMLElement) => (element.dataset["slot"] === "data-list-viewport" ? viewport : row);

  Object.defineProperty(proto, "offsetHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return height(this);
    },
  });
  Object.defineProperty(proto, "clientHeight", {
    configurable: true,
    get(this: HTMLElement) {
      return height(this);
    },
  });
  Object.defineProperty(proto, "offsetWidth", { configurable: true, get: () => 600 });
  // The viewport scrolls over the rows it holds: its content is as tall as the virtual list.
  Object.defineProperty(proto, "scrollHeight", {
    configurable: true,
    get(this: HTMLElement) {
      const list = this.querySelector<HTMLElement>("[role='list']");
      return list === null ? height(this) : Number.parseFloat(list.style.height);
    },
  });
  Object.defineProperty(proto, "scrollTop", {
    configurable: true,
    get(this: HTMLElement) {
      return scrollTops.get(this) ?? 0;
    },
    set(this: HTMLElement, value: number) {
      scrollTops.set(this, value);
    },
  });
  Object.defineProperty(proto, "scrollTo", {
    configurable: true,
    value(this: HTMLElement, options: ScrollToOptions) {
      this.scrollTop = options.top ?? this.scrollTop;
      this.dispatchEvent(new Event("scroll"));
    },
  });

  return () => {
    for (const [name, descriptor] of saved) {
      Reflect.deleteProperty(proto, name);
      if (descriptor !== undefined) {
        Object.defineProperty(proto, name, descriptor);
      }
    }
  };
}
