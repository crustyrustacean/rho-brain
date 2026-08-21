// rho-brain editor: draft autosave.
//
// Progressively enhances the document form: field values are saved to
// localStorage about a second after typing stops, keyed per document, so a
// phone tab eviction or accidental navigation never loses a draft. When a
// saved draft differs from what the server rendered, a banner offers to
// restore or discard it. The draft is cleared on successful submit, which
// navigates away via the normal form POST.
//
// Vanilla JS on purpose: this is the only client logic Datastar doesn't
// cover, and pulling a plugin for it isn't worth the bytes.
(function () {
    "use strict";

    var form = document.querySelector("form[data-draft]");
    if (!form) return;

    var key = "rho-brain-draft:" + form.getAttribute("data-draft");
    var FIELDS = ["title", "content", "tags", "metadata"];
    var SAVE_DEBOUNCE_MS = 1000;

    function readDraft() {
        try {
            var raw = window.localStorage.getItem(key);
            return raw ? JSON.parse(raw) : null;
        } catch (e) {
            return null;
        }
    }

    function writeDraft(values) {
        try {
            window.localStorage.setItem(key, JSON.stringify(values));
        } catch (e) {
            /* private mode or quota: drafts are best-effort */
        }
    }

    function clearDraft() {
        try {
            window.localStorage.removeItem(key);
        } catch (e) {}
    }

    function currentValues() {
        var values = {};
        FIELDS.forEach(function (name) {
            var field = form.elements[name];
            values[name] = field ? field.value : "";
        });
        return values;
    }

    function differs(a, b) {
        return FIELDS.some(function (name) {
            return (a[name] || "") !== (b[name] || "");
        });
    }

    function applyValues(values) {
        FIELDS.forEach(function (name) {
            var field = form.elements[name];
            if (!field) return;
            field.value = values[name] || "";
            // Let Datastar's preview action and the autosave listener see it.
            field.dispatchEvent(new Event("input", { bubbles: true }));
        });
    }

    // Save a beat after the last keystroke.
    var timer = null;
    form.addEventListener("input", function () {
        window.clearTimeout(timer);
        timer = window.setTimeout(function () {
            writeDraft(currentValues());
        }, SAVE_DEBOUNCE_MS);
    });

    // A successful submit navigates away; the draft is obsolete.
    form.addEventListener("submit", clearDraft);

    // Offer a found draft only when it differs from the server-rendered
    // values (identical values mean nothing was lost).
    var draft = readDraft();
    if (draft && differs(draft, currentValues())) {
        var banner = document.createElement("div");
        banner.className = "flash draft-banner";
        banner.setAttribute("role", "status");

        var text = document.createElement("span");
        text.textContent = "Unsaved draft found. ";
        banner.appendChild(text);

        var restore = document.createElement("button");
        restore.type = "button";
        restore.className = "btn btn-small btn-primary";
        restore.textContent = "Restore";
        restore.addEventListener("click", function () {
            applyValues(draft);
            writeDraft(draft);
            banner.remove();
        });
        banner.appendChild(restore);

        var discard = document.createElement("button");
        discard.type = "button";
        discard.className = "btn btn-small btn-secondary";
        discard.textContent = "Discard";
        discard.addEventListener("click", function () {
            clearDraft();
            banner.remove();
        });
        banner.appendChild(discard);

        form.parentNode.insertBefore(banner, form);
    }
})();
