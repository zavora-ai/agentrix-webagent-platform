/* Agentrix onboarding · the plan island (W4): pick a plan and set the AI budget as a limit.
   Prices come from the page (W3's catalogue); this only moves the pressed card, the hidden
   plan_id and the budget note. The form posts as it is, so it works without this. */
(() => {
  const form = document.getElementById("plan");
  if (!form || !document.getElementById("budget")) return;
  const $$ = s => [...document.querySelectorAll(s)];
  const range = document.getElementById("budget");
  const note = document.getElementById("budget-note");
  const out = document.getElementById("budget-out");
  const pressed = () => document.querySelector('.o-plan[aria-pressed="true"]');
  function budget() {
    const v = +range.value, inc = +(pressed()?.dataset.budget || 0);
    out.textContent = `$${v}`;
    note.textContent = v <= inc
      ? "Inside the included budget. Nothing above it is ever billed."
      : `$${v - inc} a month above the included budget, billed at cost plus a disclosed margin, and only up to this cap. Raising it later is a card.`;
  }
  function pick(btn) {
    $$(".o-plan").forEach(p => p.setAttribute("aria-pressed", String(p === btn)));
    $$("[data-plan-name]").forEach(el => el.textContent = btn.dataset.name);
    $$("[data-plan-price]").forEach(el => el.textContent = btn.dataset.price);
    $$("[data-plan-budget]").forEach(el => el.textContent = btn.dataset.budget);
    document.getElementById("plan-id").value = btn.dataset.id;
    range.max = String(+btn.dataset.budget * 4 || 20); range.value = btn.dataset.budget;
    budget();
  }
  $$(".o-plan").forEach(b => b.addEventListener("click", () => pick(b)));
  range.addEventListener("input", budget);
})();
