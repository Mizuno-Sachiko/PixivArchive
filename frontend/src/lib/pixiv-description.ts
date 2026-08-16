const allowedLinkProtocols = new Set(['http:', 'https:']);
const discardedElements = new Set([
  'AUDIO',
  'CANVAS',
  'EMBED',
  'IFRAME',
  'IMG',
  'MATH',
  'NOSCRIPT',
  'OBJECT',
  'SCRIPT',
  'STYLE',
  'SVG',
  'TEMPLATE',
  'VIDEO'
]);

export function renderPixivDescription(
  container: HTMLElement,
  source: string
): void {
  const ownerDocument = container.ownerDocument;
  const parsed = new DOMParser().parseFromString(source, 'text/html');
  const fragment = ownerDocument.createDocumentFragment();

  for (const child of parsed.body.childNodes) {
    appendSanitizedNode(fragment, child, ownerDocument);
  }

  container.replaceChildren(fragment);
}

function appendSanitizedNode(
  parent: Node,
  node: Node,
  ownerDocument: Document
): void {
  if (node.nodeType === Node.TEXT_NODE) {
    const text = normalizeText(node.textContent ?? '');
    if (text) parent.appendChild(ownerDocument.createTextNode(text));
    return;
  }
  if (!(node instanceof Element) || discardedElements.has(node.tagName)) {
    return;
  }

  if (node.tagName === 'BR') {
    if (trailingBreakCount(parent) < 2) {
      parent.appendChild(ownerDocument.createElement('br'));
    }
    return;
  }

  if (node.tagName === 'P') {
    const paragraph = ownerDocument.createElement('p');
    appendChildren(paragraph, node, ownerDocument);
    if (paragraph.textContent?.trim() || paragraph.querySelector('br, a')) {
      parent.appendChild(paragraph);
    }
    return;
  }

  if (node.tagName === 'A') {
    const href = safeHref(node.getAttribute('href'));
    if (!href) {
      appendChildren(parent, node, ownerDocument);
      return;
    }
    const link = ownerDocument.createElement('a');
    link.href = href;
    link.target = '_blank';
    link.rel = 'noopener noreferrer';
    appendChildren(link, node, ownerDocument);
    parent.appendChild(link);
    return;
  }

  appendChildren(parent, node, ownerDocument);
}

function appendChildren(
  parent: Node,
  source: Element,
  ownerDocument: Document
): void {
  for (const child of source.childNodes) {
    appendSanitizedNode(parent, child, ownerDocument);
  }
}

function normalizeText(value: string): string {
  return value.replace(/\r\n?/g, '\n').replace(/\n{3,}/g, '\n\n');
}

function safeHref(value: string | null): string | null {
  if (!value) return null;
  try {
    const url = new URL(value, 'https://www.pixiv.net');
    return allowedLinkProtocols.has(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

function trailingBreakCount(parent: Node): number {
  let count = 0;
  let current = parent.lastChild;
  while (current instanceof HTMLBRElement) {
    count += 1;
    current = current.previousSibling;
  }
  return count;
}
