import DOMPurify from "dompurify";
import { marked } from "marked";
import { projectImageUrl, projectLink } from "./projectPresentation.ts";
import { t } from "./i18n.ts";

/** Keep document structure, removing active content and external image requests. */
export function projectDescription(
  body: string,
  format: "markdown" | "html",
  base: string,
): string {
  const html =
    format === "markdown"
      ? marked.parse(body, { async: false, gfm: true })
      : body;
  const fragment = DOMPurify.sanitize(html, {
    ALLOWED_TAGS: [
      "p",
      "br",
      "hr",
      "h1",
      "h2",
      "h3",
      "h4",
      "h5",
      "h6",
      "strong",
      "b",
      "em",
      "i",
      "s",
      "del",
      "ul",
      "ol",
      "li",
      "blockquote",
      "pre",
      "code",
      "a",
      "img",
      "table",
      "thead",
      "tbody",
      "tr",
      "th",
      "td",
      "details",
      "summary",
      "div",
      "span",
      "sub",
      "sup",
      "input",
    ],
    ALLOWED_ATTR: [
      "href",
      "src",
      "alt",
      "title",
      "colspan",
      "rowspan",
      "id",
      "name",
      "class",
      "align",
      "open",
      "type",
      "checked",
      "start",
    ],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    RETURN_DOM_FRAGMENT: true,
  });
  // Remote IDs/classes never enter the app's document namespace. Local anchors
  // are resolved only within this description, without navigating the webview.
  const headings = new Map<string, number>();
  for (const element of fragment.querySelectorAll<HTMLElement>("*")) {
    const anchor =
      element.getAttribute("id") ||
      (element.tagName === "A" ? element.getAttribute("name") : null);
    if (anchor) element.dataset.projectAnchor = anchor;
    element.removeAttribute("id");
    element.removeAttribute("name");
    const language =
      element.tagName === "CODE" &&
      element.className.match(/(?:^|\s)language-([\w+-]{1,32})(?:\s|$)/)?.[1];
    element.removeAttribute("class");
    if (language) element.dataset.language = language;
    const align = element.getAttribute("align");
    element.removeAttribute("align");
    if (align && ["left", "center", "right"].includes(align))
      element.dataset.align = align;
    if (/^H[1-6]$/.test(element.tagName) && !anchor) {
      const slug =
        (element.textContent || "")
          .trim()
          .toLowerCase()
          .replace(/[^\p{L}\p{N}\p{M}_\s-]/gu, "")
          .replace(/\s/g, "-") || "section";
      const count = headings.get(slug) || 0;
      headings.set(slug, count + 1);
      element.dataset.projectAnchor = count ? `${slug}-${count}` : slug;
    }
  }
  for (const input of fragment.querySelectorAll("input")) {
    if (input.type !== "checkbox") input.remove();
    else {
      input.disabled = true;
      input.closest("li")?.classList.add("description-task");
    }
  }
  for (const anchor of fragment.querySelectorAll("a")) {
    const href = anchor.getAttribute("href");
    if (href?.startsWith("#")) continue;
    const url = href ? projectLink(href, base) : undefined;
    if (url) {
      anchor.href = url;
      anchor.rel = "noreferrer noopener";
    } else anchor.removeAttribute("href");
  }
  for (const img of fragment.querySelectorAll("img")) {
    const url = projectImageUrl(img.getAttribute("src") || "");
    img.removeAttribute("src");
    if (url) {
      img.dataset.projectImage = url;
      img.loading = "lazy";
      img.decoding = "async";
      if (!img.closest("a[href]")) {
        const preview = document.createElement("button");
        preview.type = "button";
        preview.className = "description-image";
        preview.dataset.previewImage = "";
        preview.disabled = true;
        preview.setAttribute(
          "aria-label",
          img.alt ? `${t("查看大图")}: ${img.alt}` : t("查看大图"),
        );
        img.before(preview);
        preview.append(img);
      }
    } else img.replaceWith(document.createTextNode(img.alt));
  }
  for (const pre of fragment.querySelectorAll("pre")) {
    const code = pre.querySelector("code");
    if (!code) continue;
    const block = document.createElement("div");
    block.className = "description-code";
    const toolbar = document.createElement("div");
    toolbar.className = "description-code-tools";
    const language = document.createElement("span");
    language.textContent = code.dataset.language || "";
    const copy = document.createElement("button");
    copy.type = "button";
    copy.dataset.copyCode = "";
    copy.textContent = t("复制代码");
    copy.setAttribute("aria-live", "polite");
    toolbar.append(language, copy);
    pre.before(block);
    block.append(toolbar, pre);
    pre.tabIndex = 0;
    pre.setAttribute("aria-label", t("代码"));
  }
  const container = document.createElement("div");
  container.append(fragment);
  return container.innerHTML;
}
