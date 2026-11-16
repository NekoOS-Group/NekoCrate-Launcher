import { ref } from "vue";

// Hash routes: #download (default), #features, #docs, #docs/<article>.
export const pages = ["download", "features", "docs"];

function parse() {
  const [page, sub] = decodeURIComponent(location.hash.slice(1)).split("/");
  return pages.includes(page)
    ? { page, sub: sub || "" }
    : { page: "download", sub: "" };
}

export const route = ref(parse());

export const href = (page, sub) => `#${sub ? `${page}/${sub}` : page}`;

window.addEventListener("hashchange", () => {
  route.value = parse();
});
