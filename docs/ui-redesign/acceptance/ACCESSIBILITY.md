# Accessibility Acceptance

- Every interactive control has an accessible name and correct role.
- Landmark names are unique where multiple landmarks of the same type exist.
- Keyboard focus is visible in Dark and Light themes.
- Dialogs and popovers manage focus and Escape behavior correctly.
- Tables expose headers, selection and sort state.
- Status is not conveyed by color alone.
- Contrast meets WCAG AA: 4.5:1 normal text and 3:1 large text.
- Respect `prefers-reduced-motion`.
- Run Playwright + axe on core routes in both themes; serious and critical violations must be zero.
- Current covered axe gate passed in the 71/71 run; full five-viewport and all-state manual coverage remains an M7 requirement.
