import { LoginAlternatives, login, useSession } from "@/features/auth";
import {
  Button,
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
} from "@/shared/components";
import { AppBrand } from "@/widgets/AppShell";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router";
import { useForm } from "react-hook-form";
import { z } from "zod";

const formSchema = z.object({ email: z.email(), password: z.string().min(1) });
type FormValues = z.infer<typeof formSchema>;

/** Displays local and external connector login choices. @returns The accessible login page. */
export function LoginPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const session = useSession();
  const form = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: { email: "", password: "" },
  });
  const mutation = useMutation({
    mutationFn: ({ email, password }: FormValues) => login(email, password),
    onSuccess: async (value) => {
      queryClient.setQueryData(["session"], value);
      await navigate("/");
    },
  });
  const connectors = session.data?.connectors ?? [];

  return (
    <main className="grid min-h-screen bg-background lg:grid-cols-[44%_56%]">
      <section className="hidden items-center justify-center bg-sidebar px-12 text-sidebar-foreground lg:flex">
        <div className="flex max-w-sm flex-col items-center gap-8 text-center">
          <AppBrand size="hero" />
          <p className="text-base leading-6 text-sidebar-foreground/85">
            {t("auth.brandDescription")}
          </p>
        </div>
      </section>

      <section className="flex min-h-screen items-center justify-center px-6 py-10 sm:px-10">
        <div className="w-full max-w-[18.25rem]">
          <header className="mb-7 space-y-1">
            <h1 className="text-2xl font-extrabold tracking-tight">
              {t("auth.loginTitle")}
            </h1>
            <p className="text-sm text-muted-foreground">
              {t("auth.loginDescription")}
            </p>
          </header>

          <form
            className="flex flex-col gap-5"
            id="login-form"
            onSubmit={(event) => {
              void form.handleSubmit((value) => {
                mutation.mutate(value);
              })(event);
            }}
          >
            <FieldGroup>
              <Field data-invalid={Boolean(form.formState.errors.email)}>
                <FieldLabel htmlFor="email">{t("auth.email")}</FieldLabel>
                <Input
                  aria-invalid={Boolean(form.formState.errors.email)}
                  autoComplete="email"
                  id="email"
                  placeholder={t("auth.emailPlaceholder")}
                  type="email"
                  {...form.register("email")}
                />
                <FieldError errors={[form.formState.errors.email]} />
              </Field>
              <Field data-invalid={Boolean(form.formState.errors.password)}>
                <FieldLabel htmlFor="password">{t("auth.password")}</FieldLabel>
                <Input
                  aria-invalid={Boolean(form.formState.errors.password)}
                  autoComplete="current-password"
                  id="password"
                  type="password"
                  {...form.register("password")}
                />
                <FieldError errors={[form.formState.errors.password]} />
              </Field>
              {mutation.isError ? (
                <FieldError>{t("auth.loginFailed")}</FieldError>
              ) : null}
            </FieldGroup>
            <Button
              className="w-full"
              disabled={mutation.isPending}
              type="submit"
            >
              {t("auth.signIn")}
            </Button>
          </form>

          <LoginAlternatives connectors={connectors} />
        </div>
      </section>
    </main>
  );
}
