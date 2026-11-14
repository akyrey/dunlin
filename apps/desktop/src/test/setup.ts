import "@testing-library/jest-dom/vitest";

// Editor tabs are saved in localStorage; every test starts without saved tabs.
beforeEach(() => {
  localStorage.clear();
});
