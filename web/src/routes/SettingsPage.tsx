import { useEffect, useState } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { getStoredTheme, applyTheme, THEME_OPTIONS, type ThemeId } from "../theme";
import { setAppLanguage, SUPPORTED_LANGUAGES, type AppLanguage } from "../i18n";

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const [theme, setTheme] = useState<ThemeId>(() => getStoredTheme());
  const language = (i18n.resolvedLanguage ?? i18n.language) as AppLanguage;

  useEffect(() => {
    const onThemeChange = (event: Event) => {
      const next = (event as CustomEvent<ThemeId>).detail;
      if (next === "rip-dark" || next === "rip-light") setTheme(next);
    };
    window.addEventListener("rip-theme-change", onThemeChange);
    return () => window.removeEventListener("rip-theme-change", onThemeChange);
  }, []);

  const handleThemeChange = (value: ThemeId) => {
    applyTheme(value);
    setTheme(value);
  };

  return (
    <div className="settings-page">
      <header className="settings-header">
        <div>
          <p className="settings-eyebrow">{t("settings.general")}</p>
          <h2>{t("settings.title")}</h2>
          <p className="settings-description">{t("settings.description")}</p>
        </div>
      </header>

      <section className="settings-section" aria-labelledby="settings-language-title">
        <div className="settings-section-copy">
          <h3 id="settings-language-title">{t("settings.language")}</h3>
          <p>{t("settings.languageDescription")}</p>
        </div>
        <div className="settings-section-control">
          <label htmlFor="settings-language">{t("settings.language")}</label>
          <select
            id="settings-language"
            className="settings-select"
            value={language}
            onChange={(event) => void setAppLanguage(event.target.value as AppLanguage)}
          >
            {SUPPORTED_LANGUAGES.map((item) => (
              <option key={item.code} value={item.code}>
                {item.label}
              </option>
            ))}
          </select>
          <span className="settings-saved">{t("settings.savedLocally")}</span>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-theme-title">
        <div className="settings-section-copy">
          <h3 id="settings-theme-title">{t("settings.theme")}</h3>
          <p>{t("settings.themeDescription")}</p>
        </div>
        <div className="settings-section-control">
          <label htmlFor="settings-theme">{t("settings.theme")}</label>
          <select
            id="settings-theme"
            className="settings-select"
            value={theme}
            onChange={(event) => handleThemeChange(event.target.value as ThemeId)}
          >
            <option value={THEME_OPTIONS[0].id}>{t("settings.dark")}</option>
            <option value={THEME_OPTIONS[1].id}>{t("settings.light")}</option>
          </select>
          <span className="settings-saved">{t("settings.savedLocally")}</span>
        </div>
      </section>

      <section className="settings-section" aria-labelledby="settings-density-title">
        <div className="settings-section-copy">
          <h3 id="settings-density-title">{t("settings.density")}</h3>
          <p>{t("settings.densityDescription")}</p>
        </div>
        <span className="settings-coming-soon">{t("common.comingSoon")}</span>
      </section>

      <section className="settings-related" aria-labelledby="settings-system-title">
        <h3 id="settings-system-title">{t("settings.system")}</h3>
        <div className="settings-related-links">
          <Link to="/runtime">{t("settings.runtime")}</Link>
          <Link to="/settings/wsw-proxy">{t("settings.proxyStatus")}</Link>
          <Link to="/settings/wsw-cert">{t("settings.certStatus")}</Link>
        </div>
      </section>
    </div>
  );
}
