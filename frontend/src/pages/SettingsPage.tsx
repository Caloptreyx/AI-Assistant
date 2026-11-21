import { faGauge, faKey, faMessage, faRobot, faToggleOn, faUserShield } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { SimpleGrid } from '@mantine/core';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import { AdminCan } from '@/elements/Can.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import Checkbox from '@/elements/input/Checkbox.tsx';
import NumberInput from '@/elements/input/NumberInput.tsx';
import PasswordInput from '@/elements/input/PasswordInput.tsx';
import Select from '@/elements/input/Select.tsx';
import Switch from '@/elements/input/Switch.tsx';
import TextArea from '@/elements/input/TextArea.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import SegmentedControl from '@/elements/layout/SegmentedControl.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useAdminCan } from '@/plugins/usePermissions.ts';
import { useToast } from '@/providers/ToastProvider.tsx';
import {
  type ConnectionTest,
  getSettings,
  PACKAGE,
  type Settings,
  type SettingsUpdate,
  testSettings,
  updateSettings,
} from '../api.ts';
import { CHAT_DISPLAYS } from '../lib/display.ts';
import { useExtTranslations } from '../translations.ts';

const settingsKey = [PACKAGE, 'settings'] as const;

type NumberField =
  | 'requestsPerHour'
  | 'dailyTokenLimit'
  | 'maxToolSteps'
  | 'maxOutputTokens'
  | 'toolOutputLimit'
  | 'historyLimit'
  | 'retentionDays';

const NUMBER_FIELDS: { field: NumberField; min: number; max: number }[] = [
  { field: 'requestsPerHour', min: 0, max: 10_000 },
  { field: 'dailyTokenLimit', min: 0, max: 1_000_000_000 },
  { field: 'maxToolSteps', min: 1, max: 25 },
  { field: 'maxOutputTokens', min: 256, max: 64_000 },
  { field: 'toolOutputLimit', min: 1_000, max: 100_000 },
  { field: 'historyLimit', min: 4, max: 200 },
  { field: 'retentionDays', min: 0, max: 3650 },
];

/** A new key typed by the admin, or the instruction to remove the stored one. */
function keyChange(typed: string, clear: boolean): string | undefined {
  if (clear) {
    return '';
  }
  return typed.trim() || undefined;
}

export default function SettingsPage() {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const canRead = useAdminCan('extensions.read');
  const canManage = useAdminCan('extensions.manage');

  const query = useQuery({ queryKey: settingsKey, queryFn: getSettings, enabled: canRead });
  const [form, setForm] = useState<Settings | null>(null);
  const [openaiKey, setOpenaiKey] = useState('');
  const [anthropicKey, setAnthropicKey] = useState('');
  const [clearOpenaiKey, setClearOpenaiKey] = useState(false);
  const [clearAnthropicKey, setClearAnthropicKey] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<ConnectionTest | null>(null);

  useEffect(() => {
    if (query.data) {
      setForm(query.data);
    }
  }, [query.data]);

  if (query.isLoading) {
    return (
      <Group justify='center' py='lg'>
        <Spinner />
      </Group>
    );
  }
  if (query.error || !form) {
    return <Alert color='red'>{query.error ? httpErrorToHuman(query.error) : null}</Alert>;
  }

  const update = (patch: Partial<Settings>) => setForm({ ...form, ...patch });
  const secret = (stored: Settings['openaiApiKey']) =>
    stored.set
      ? stored.hint
        ? tExt('settings.provider.apiKeyStored', { hint: stored.hint })
        : tExt('settings.provider.apiKeyStoredNoHint', {})
      : tExt('settings.provider.apiKeyNone', {});

  const save = async () => {
    setSaving(true);
    const change: SettingsUpdate = {
      ...form,
      openaiApiKey: keyChange(openaiKey, clearOpenaiKey),
      anthropicApiKey: keyChange(anthropicKey, clearAnthropicKey),
    };
    try {
      const saved = await updateSettings(change);
      setForm(saved);
      setOpenaiKey('');
      setAnthropicKey('');
      setClearOpenaiKey(false);
      setClearAnthropicKey(false);
      queryClient.setQueryData(settingsKey, saved);
      queryClient.invalidateQueries({ queryKey: [PACKAGE] });
      addToast(tExt('settings.saved', {}), 'success');
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setSaving(false);
    }
  };

  const runTest = async () => {
    setTesting(true);
    setTest(null);
    try {
      setTest(await testSettings());
    } catch (error) {
      setTest({ ok: false, error: httpErrorToHuman(error) });
    } finally {
      setTesting(false);
    }
  };

  const openai = form.provider === 'openai';

  return (
    <Stack>
      <TitleCard title={tExt('settings.provider.title', {})} icon={<FontAwesomeIcon icon={faRobot} />}>
        <Stack>
          <Select
            label={tExt('settings.provider.kind', {})}
            data={[
              { value: 'openai', label: tExt('settings.provider.openai', {}) },
              { value: 'anthropic', label: tExt('settings.provider.anthropic', {}) },
            ]}
            value={form.provider}
            onChange={(value) => value && update({ provider: value as Settings['provider'] })}
            allowDeselect={false}
            disabled={!canManage}
          />
          {openai ? (
            <>
              <TextInput
                label={tExt('settings.provider.baseUrl', {})}
                description={tExt('settings.provider.baseUrlDescription', {})}
                value={form.openaiBaseUrl}
                onChange={(event) => update({ openaiBaseUrl: event.currentTarget.value })}
                disabled={!canManage}
              />
              <PasswordInput
                label={tExt('settings.provider.apiKey', {})}
                description={`${tExt('settings.provider.apiKeyDescription', {})} ${secret(form.openaiApiKey)}`}
                leftSection={<FontAwesomeIcon icon={faKey} />}
                value={openaiKey}
                onChange={(event) => setOpenaiKey(event.currentTarget.value)}
                disabled={!canManage || clearOpenaiKey}
              />
              {form.openaiApiKey.set && (
                <Checkbox
                  label={tExt('settings.provider.clearKey', {})}
                  checked={clearOpenaiKey}
                  onChange={(event) => setClearOpenaiKey(event.currentTarget.checked)}
                  disabled={!canManage}
                />
              )}
              <TextInput
                label={tExt('settings.provider.model', {})}
                description={tExt('settings.provider.modelDescription', {})}
                value={form.openaiModel}
                onChange={(event) => update({ openaiModel: event.currentTarget.value })}
                disabled={!canManage}
              />
            </>
          ) : (
            <>
              <PasswordInput
                label={tExt('settings.provider.apiKey', {})}
                description={`${tExt('settings.provider.apiKeyDescription', {})} ${secret(form.anthropicApiKey)}`}
                leftSection={<FontAwesomeIcon icon={faKey} />}
                value={anthropicKey}
                onChange={(event) => setAnthropicKey(event.currentTarget.value)}
                disabled={!canManage || clearAnthropicKey}
              />
              {form.anthropicApiKey.set && (
                <Checkbox
                  label={tExt('settings.provider.clearKey', {})}
                  checked={clearAnthropicKey}
                  onChange={(event) => setClearAnthropicKey(event.currentTarget.checked)}
                  disabled={!canManage}
                />
              )}
              <TextInput
                label={tExt('settings.provider.model', {})}
                description={tExt('settings.provider.modelDescription', {})}
                value={form.anthropicModel}
                onChange={(event) => update({ anthropicModel: event.currentTarget.value })}
                disabled={!canManage}
              />
            </>
          )}
          {test && (
            <Alert color={test.ok ? 'green' : 'red'}>
              {test.ok
                ? tExt('settings.provider.testOk', { reply: test.reply ?? '' })
                : tExt('settings.provider.testFailed', { error: test.error ?? '' })}
            </Alert>
          )}
        </Stack>
      </TitleCard>

      <TitleCard title={tExt('settings.userKeys.title', {})} icon={<FontAwesomeIcon icon={faUserShield} />}>
        <Stack>
          <div>
            <Text size='sm' fw={500} mb={6}>
              {tExt('settings.userKeys.mode', {})}
            </Text>
            <SegmentedControl
              data={(['off', 'allowed', 'required'] as const).map((mode) => ({
                value: mode,
                label: tExt(`settings.userKeys.${mode}`, {}),
              }))}
              value={form.userKeys}
              onChange={(value) => update({ userKeys: value as Settings['userKeys'] })}
              disabled={!canManage}
            />
            <Text size='xs' c='dimmed' mt={6}>
              {tExt(`settings.userKeys.${form.userKeys}Description`, {})}
            </Text>
          </div>
          <Switch
            label={tExt('settings.userKeys.customUrls', {})}
            description={tExt('settings.userKeys.customUrlsDescription', {})}
            checked={form.allowCustomBaseUrls}
            onChange={(event) => update({ allowCustomBaseUrls: event.currentTarget.checked })}
            disabled={!canManage || form.userKeys === 'off'}
          />
        </Stack>
      </TitleCard>

      <TitleCard title={tExt('settings.access.title', {})} icon={<FontAwesomeIcon icon={faToggleOn} />}>
        <Stack>
          <Switch
            label={tExt('settings.access.serverChat', {})}
            description={tExt('settings.access.serverChatDescription', {})}
            checked={form.serverChatEnabled}
            onChange={(event) => update({ serverChatEnabled: event.currentTarget.checked })}
            disabled={!canManage}
          />
          <Switch
            label={tExt('settings.access.adminChat', {})}
            description={tExt('settings.access.adminChatDescription', {})}
            checked={form.adminChatEnabled}
            onChange={(event) => update({ adminChatEnabled: event.currentTarget.checked })}
            disabled={!canManage}
          />
          <div>
            <Text size='sm' fw={500} mb={6}>
              {tExt('settings.access.display', {})}
            </Text>
            <SegmentedControl
              data={CHAT_DISPLAYS.map((display) => ({
                value: display,
                label: tExt(`settings.access.display_${display}`, {}),
              }))}
              value={form.chatDisplay}
              onChange={(value) => update({ chatDisplay: value as Settings['chatDisplay'] })}
              disabled={!canManage}
            />
            <Text size='xs' c='dimmed' mt={6}>
              {tExt(`settings.access.display_${form.chatDisplay}Description`, {})}{' '}
              {tExt('settings.access.displayReload', {})}
            </Text>
          </div>
        </Stack>
      </TitleCard>

      <TitleCard title={tExt('settings.limits.title', {})} icon={<FontAwesomeIcon icon={faGauge} />}>
        <SimpleGrid cols={{ base: 1, md: 2 }}>
          {NUMBER_FIELDS.map(({ field, min, max }) => (
            <NumberInput
              key={field}
              label={tExt(`settings.limits.${field}`, {})}
              description={tExt(`settings.limits.${field}Description`, {})}
              value={form[field]}
              min={min}
              max={max}
              allowDecimal={false}
              thousandSeparator=' '
              onChange={(value) => {
                const parsed = typeof value === 'number' ? value : Number.parseInt(value, 10);
                if (Number.isFinite(parsed)) {
                  const patch: Partial<Settings> = {};
                  patch[field] = Math.min(max, Math.max(min, Math.trunc(parsed)));
                  update(patch);
                }
              }}
              disabled={!canManage}
            />
          ))}
        </SimpleGrid>
      </TitleCard>

      <TitleCard title={tExt('settings.prompt.title', {})} icon={<FontAwesomeIcon icon={faMessage} />}>
        <TextArea
          label={tExt('settings.prompt.instructions', {})}
          description={tExt('settings.prompt.instructionsDescription', {})}
          value={form.instructions}
          onChange={(event) => update({ instructions: event.currentTarget.value })}
          autosize
          minRows={3}
          maxRows={12}
          maxLength={8000}
          disabled={!canManage}
        />
      </TitleCard>

      <Group justify='space-between'>
        <Text size='sm' c='dimmed'>
          {PACKAGE}
        </Text>
        <Group>
          <AdminCan action='extensions.manage'>
            <Button variant='default' onClick={runTest} loading={testing}>
              {tExt('settings.provider.test', {})}
            </Button>
          </AdminCan>
          <AdminCan action='extensions.manage' cantSave>
            <Button onClick={save} loading={saving}>
              {tExt('settings.save', {})}
            </Button>
          </AdminCan>
        </Group>
      </Group>
    </Stack>
  );
}
