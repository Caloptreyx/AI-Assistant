import { faCircleCheck, faKey, faPlug, faTrash } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { httpErrorToHuman } from '@/api/axios.ts';
import Button from '@/elements/buttons/Button.tsx';
import AccountContentContainer from '@/elements/containers/AccountContentContainer.tsx';
import Badge from '@/elements/data-display/Badge.tsx';
import TitleCard from '@/elements/data-display/TitleCard.tsx';
import Alert from '@/elements/feedback/Alert.tsx';
import Spinner from '@/elements/feedback/Spinner.tsx';
import PasswordInput from '@/elements/input/PasswordInput.tsx';
import Select from '@/elements/input/Select.tsx';
import TextInput from '@/elements/input/TextInput.tsx';
import Group from '@/elements/layout/Group.tsx';
import Stack from '@/elements/layout/Stack.tsx';
import ConfirmationModal from '@/elements/modals/ConfirmationModal.tsx';
import Text from '@/elements/typography/Text.tsx';
import { useToast } from '@/providers/ToastProvider.tsx';
import {
  type ConnectionTest,
  deleteUserKey,
  getUserKey,
  PACKAGE,
  saveUserKey,
  testUserKey,
  type UserKeyState,
} from '../api.ts';
import { useExtTranslations } from '../translations.ts';

const CUSTOM = 'custom';
const userKeyQuery = [PACKAGE, 'user-key'] as const;

export default function AccountKeyPage() {
  const { t: tExt } = useExtTranslations();
  const { addToast } = useToast();
  const queryClient = useQueryClient();
  const query = useQuery({ queryKey: userKeyQuery, queryFn: getUserKey });

  const [preset, setPreset] = useState('openai');
  const [model, setModel] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [apiKey, setApiKey] = useState('');
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [test, setTest] = useState<ConnectionTest | null>(null);
  const [removing, setRemoving] = useState(false);

  const fill = (state: UserKeyState) => {
    setPreset(state.key?.preset ?? state.presets[0]?.id ?? 'openai');
    setModel(state.key?.model ?? '');
    setBaseUrl(state.key?.base_url ?? '');
    setApiKey('');
  };

  useEffect(() => {
    if (query.data) {
      fill(query.data);
    }
  }, [query.data]);

  if (query.isLoading) {
    return (
      <AccountContentContainer title={tExt('account.title', {})}>
        <Group justify='center' py='lg'>
          <Spinner />
        </Group>
      </AccountContentContainer>
    );
  }

  const data = query.data;
  if (query.error || !data) {
    return (
      <AccountContentContainer title={tExt('account.title', {})}>
        <Alert color='red'>{query.error ? httpErrorToHuman(query.error) : null}</Alert>
      </AccountContentContainer>
    );
  }

  const stored = data.key;
  const chosen = data.presets.find((candidate) => candidate.id === preset);
  const custom = preset === CUSTOM;
  const off = data.mode === 'off';

  const applied = (state: UserKeyState) => {
    queryClient.setQueryData(userKeyQuery, state);
    // The chat pages show whose key is used.
    queryClient.invalidateQueries({ queryKey: [PACKAGE] });
  };

  const save = async () => {
    setSaving(true);
    setTest(null);
    try {
      applied(
        await saveUserKey({
          preset,
          base_url: custom ? baseUrl.trim() : undefined,
          model: model.trim() || chosen?.default_model || '',
          api_key: apiKey.trim() || undefined,
        }),
      );
      addToast(tExt('account.saved', {}), 'success');
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
      setTest(await testUserKey());
    } catch (error) {
      setTest({ ok: false, error: httpErrorToHuman(error) });
    } finally {
      setTesting(false);
    }
  };

  const remove = async () => {
    try {
      applied(await deleteUserKey());
      setTest(null);
      addToast(tExt('account.removed', {}), 'success');
    } catch (error) {
      addToast(httpErrorToHuman(error), 'error');
    } finally {
      setRemoving(false);
    }
  };

  const presetOptions = [
    ...data.presets.map((candidate) => ({ value: candidate.id, label: candidate.name })),
    ...(data.allow_custom_base_urls || stored?.preset === CUSTOM
      ? [{ value: CUSTOM, label: tExt('account.custom', {}) }]
      : []),
  ];
  const presetName = (id: string) =>
    id === CUSTOM ? tExt('account.custom', {}) : (data.presets.find((candidate) => candidate.id === id)?.name ?? id);

  return (
    <AccountContentContainer title={tExt('account.title', {})} subtitle={tExt('account.subtitle', {})}>
      <ConfirmationModal
        opened={removing}
        onClose={() => setRemoving(false)}
        title={tExt('account.removeTitle', {})}
        confirm={tExt('account.remove', {})}
        onConfirmed={remove}
      >
        {data.mode === 'required' ? tExt('account.removeRequired', {}) : tExt('account.removeAllowed', {})}
      </ConfirmationModal>

      <Stack>
        {off ? (
          <Alert color='yellow'>{tExt('account.off', {})}</Alert>
        ) : (
          <Alert color='blue' icon={<FontAwesomeIcon icon={faKey} />}>
            {data.mode === 'required' ? tExt('account.required', {}) : tExt('account.allowed', {})}
          </Alert>
        )}

        {stored && (
          <TitleCard title={tExt('account.current', {})} icon={<FontAwesomeIcon icon={faCircleCheck} />}>
            <Stack gap='sm'>
              <Group gap='xs'>
                <Badge variant='light' color='teal'>
                  {presetName(stored.preset)}
                </Badge>
                <Badge variant='default' tt='none'>
                  {stored.model}
                </Badge>
                {stored.api_key.hint && (
                  <Badge variant='default' tt='none' className='font-mono'>
                    {stored.api_key.hint}
                  </Badge>
                )}
              </Group>
              {stored.base_url && (
                <Text size='sm' c='dimmed' className='break-all'>
                  {stored.base_url}
                </Text>
              )}
              <Text size='xs' c='dimmed'>
                {tExt('account.updated', { date: new Date(stored.updated).toLocaleString() })}
              </Text>
              {test && (
                <Alert color={test.ok ? 'green' : 'red'}>
                  {test.ok
                    ? tExt('settings.provider.testOk', { reply: test.reply ?? '' })
                    : tExt('settings.provider.testFailed', { error: test.error ?? '' })}
                </Alert>
              )}
              <Group gap='xs'>
                <Button
                  variant='default'
                  leftSection={<FontAwesomeIcon icon={faPlug} />}
                  onClick={runTest}
                  loading={testing}
                  disabled={off}
                >
                  {tExt('settings.provider.test', {})}
                </Button>
                <Button
                  color='red'
                  variant='light'
                  leftSection={<FontAwesomeIcon icon={faTrash} />}
                  onClick={() => setRemoving(true)}
                >
                  {tExt('account.remove', {})}
                </Button>
              </Group>
            </Stack>
          </TitleCard>
        )}

        {!off && (
          <TitleCard
            title={stored ? tExt('account.change', {}) : tExt('account.add', {})}
            icon={<FontAwesomeIcon icon={faKey} />}
          >
            <Stack>
              <Select
                label={tExt('account.provider', {})}
                data={presetOptions}
                value={preset}
                onChange={(value) => value && setPreset(value)}
                allowDeselect={false}
              />
              {custom && (
                <TextInput
                  label={tExt('settings.provider.baseUrl', {})}
                  description={tExt('account.baseUrlDescription', {})}
                  placeholder='https://api.example.com/v1'
                  value={baseUrl}
                  onChange={(event) => setBaseUrl(event.currentTarget.value)}
                  required
                />
              )}
              <PasswordInput
                label={tExt('settings.provider.apiKey', {})}
                description={tExt('account.keyDescription', {})}
                placeholder={stored?.api_key.hint ? tExt('account.keepKey', { hint: stored.api_key.hint }) : undefined}
                leftSection={<FontAwesomeIcon icon={faKey} />}
                value={apiKey}
                onChange={(event) => setApiKey(event.currentTarget.value)}
                autoComplete='off'
                required={!stored}
              />
              <TextInput
                label={tExt('settings.provider.model', {})}
                description={tExt('settings.provider.modelDescription', {})}
                placeholder={chosen?.default_model}
                value={model}
                onChange={(event) => setModel(event.currentTarget.value)}
              />
              <Group justify='flex-end'>
                <Button
                  onClick={save}
                  loading={saving}
                  disabled={(!stored && !apiKey.trim()) || (custom && !baseUrl.trim())}
                >
                  {tExt('account.save', {})}
                </Button>
              </Group>
            </Stack>
          </TitleCard>
        )}
      </Stack>
    </AccountContentContainer>
  );
}
