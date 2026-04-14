/**
 * Settings
 * Animated overlay that hydrates from the backend `AppConfig`, lets the user
 * edit a curated subset of fields, and persists via `save_config`.
 */

import { useEffect, useState } from 'react';
import { AnimatePresence, motion } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { useRenStore } from '../store';
import styles from './Settings.module.css';

/**
 * Mirror of the Rust `AppConfig` serde shape. Only the fields the settings
 * panel touches are listed here — the rest flows through opaque and is
 * round-tripped back via `save_config`.
 */
interface AppConfigPayload {
  sample_rate: number;
  channels: number;
  wake_sensitivity: number;
  conversation_timeout_secs: number;
  tts_voice: string;
  ollama_port: number | null;
  ollama_model: string;
  brave_api_key: string | null;
  location: string | null;
  autostart: boolean;
}

type SaveStatus = 'idle' | 'saving' | 'saved' | 'error';

export const Settings = () => {
  const { t } = useTranslation();
  const open = useRenStore((s) => s.settingsOpen);
  const setSettingsOpen = useRenStore((s) => s.setSettingsOpen);

  const [config, setConfig] = useState<AppConfigPayload | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [status, setStatus] = useState<SaveStatus>('idle');

  // Load the snapshot each time the panel opens so external edits to
  // config.json aren't stale; the cost of a single IPC call is negligible.
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoadError(null);
    setStatus('idle');
    invoke<AppConfigPayload>('get_config')
      .then((fresh) => {
        if (!cancelled) setConfig(fresh);
      })
      .catch((err) => {
        if (!cancelled) setLoadError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  // ESC closes — consistent with every overlay pattern on this surface.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setSettingsOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [open, setSettingsOpen]);

  const patch = <K extends keyof AppConfigPayload>(key: K, value: AppConfigPayload[K]) => {
    setConfig((prev) => (prev ? { ...prev, [key]: value } : prev));
    if (status === 'saved') setStatus('idle');
  };

  const handleSave = async () => {
    if (!config) return;
    setStatus('saving');
    try {
      await invoke('save_config', { newConfig: config });
      setStatus('saved');
    } catch (err) {
      setLoadError(String(err));
      setStatus('error');
    }
  };

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          className={styles.backdrop}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          onClick={() => setSettingsOpen(false)}
        >
          <motion.section
            className={styles.panel}
            role="dialog"
            aria-modal="true"
            aria-label={t('settings.title')}
            initial={{ x: '100%' }}
            animate={{ x: 0 }}
            exit={{ x: '100%' }}
            transition={{ type: 'tween', ease: [0.25, 0.1, 0.25, 1], duration: 0.25 }}
            onClick={(e) => e.stopPropagation()}
          >
            <header className={styles.header}>
              <h2 className={styles.title}>{t('settings.title')}</h2>
              <button
                type="button"
                className={styles.closeButton}
                onClick={() => setSettingsOpen(false)}
                aria-label={t('settings.close')}
              >
                ×
              </button>
            </header>

            <div className={styles.body}>
              {loadError && <p className={styles.error}>{loadError}</p>}
              {!config && !loadError && <p className={styles.loading}>…</p>}

              {config && (
                <form
                  className={styles.form}
                  onSubmit={(e) => {
                    e.preventDefault();
                    handleSave();
                  }}
                >
                  <fieldset className={styles.section}>
                    <legend className={styles.sectionTitle}>
                      {t('settings.sections.voice')}
                    </legend>

                    <label className={styles.field}>
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.wake_sensitivity.label')}
                      </span>
                      <div className={styles.sliderRow}>
                        <input
                          className={styles.slider}
                          type="range"
                          min={0}
                          max={1}
                          step={0.05}
                          value={config.wake_sensitivity}
                          onChange={(e) =>
                            patch('wake_sensitivity', Number(e.target.value))
                          }
                        />
                        <span className={styles.sliderValue}>
                          {config.wake_sensitivity.toFixed(2)}
                        </span>
                      </div>
                      <span className={styles.fieldHelp}>
                        {t('settings.fields.wake_sensitivity.help')}
                      </span>
                    </label>

                    <label className={styles.field}>
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.tts_voice.label')}
                      </span>
                      <input
                        className={styles.input}
                        type="text"
                        value={config.tts_voice}
                        onChange={(e) => patch('tts_voice', e.target.value)}
                      />
                      <span className={styles.fieldHelp}>
                        {t('settings.fields.tts_voice.help')}
                      </span>
                    </label>
                  </fieldset>

                  <fieldset className={styles.section}>
                    <legend className={styles.sectionTitle}>
                      {t('settings.sections.conversation')}
                    </legend>

                    <label className={styles.field}>
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.conversation_timeout.label')}
                      </span>
                      <input
                        className={styles.input}
                        type="number"
                        min={5}
                        max={600}
                        value={config.conversation_timeout_secs}
                        onChange={(e) =>
                          patch(
                            'conversation_timeout_secs',
                            Math.max(5, Number(e.target.value) || 0),
                          )
                        }
                      />
                      <span className={styles.fieldHelp}>
                        {t('settings.fields.conversation_timeout.help')}
                      </span>
                    </label>
                  </fieldset>

                  <fieldset className={styles.section}>
                    <legend className={styles.sectionTitle}>
                      {t('settings.sections.location')}
                    </legend>

                    <label className={styles.field}>
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.location.label')}
                      </span>
                      <input
                        className={styles.input}
                        type="text"
                        placeholder={t('settings.fields.location.placeholder') ?? ''}
                        value={config.location ?? ''}
                        onChange={(e) =>
                          patch('location', e.target.value.trim() === '' ? null : e.target.value)
                        }
                      />
                      <span className={styles.fieldHelp}>
                        {t('settings.fields.location.help')}
                      </span>
                    </label>
                  </fieldset>

                  <fieldset className={styles.section}>
                    <legend className={styles.sectionTitle}>
                      {t('settings.sections.apis')}
                    </legend>

                    <label className={styles.field}>
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.brave_api_key.label')}
                      </span>
                      <input
                        className={styles.input}
                        type="password"
                        placeholder={t('settings.fields.brave_api_key.placeholder') ?? ''}
                        value={config.brave_api_key ?? ''}
                        onChange={(e) =>
                          patch(
                            'brave_api_key',
                            e.target.value.trim() === '' ? null : e.target.value,
                          )
                        }
                      />
                      <span className={styles.fieldHelp}>
                        {t('settings.fields.brave_api_key.help')}
                      </span>
                    </label>
                  </fieldset>

                  <fieldset className={styles.section}>
                    <legend className={styles.sectionTitle}>
                      {t('settings.sections.system')}
                    </legend>

                    <label className={styles.fieldInline}>
                      <input
                        type="checkbox"
                        checked={config.autostart}
                        onChange={(e) => patch('autostart', e.target.checked)}
                      />
                      <span className={styles.fieldLabel}>
                        {t('settings.fields.autostart.label')}
                      </span>
                    </label>
                    <span className={styles.fieldHelp}>
                      {t('settings.fields.autostart.help')}
                    </span>
                  </fieldset>

                  <div className={styles.soon}>
                    <strong className={styles.soonTitle}>{t('settings.soon.title')}</strong>
                    <span className={styles.soonBody}>{t('settings.soon.body')}</span>
                  </div>

                  <footer className={styles.footer}>
                    <button
                      type="button"
                      className={styles.secondaryButton}
                      onClick={() => setSettingsOpen(false)}
                    >
                      {t('settings.cancel')}
                    </button>
                    <button
                      type="submit"
                      className={styles.primaryButton}
                      disabled={status === 'saving'}
                    >
                      {status === 'saving'
                        ? t('settings.saving')
                        : status === 'saved'
                          ? t('settings.saved')
                          : t('settings.save')}
                    </button>
                  </footer>
                </form>
              )}
            </div>
          </motion.section>
        </motion.div>
      )}
    </AnimatePresence>
  );
};
