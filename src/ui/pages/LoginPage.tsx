import { login, useSession } from "@/features/auth";
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
  Field,
  FieldError,
  FieldGroup,
  FieldLabel,
  Input,
  Separator,
} from "@/shared/components";
import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router";
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
  return (
    <main className="mx-auto flex min-h-screen max-w-md items-center px-6 py-10">
      <Card className="w-full">
        <CardHeader>
          <CardTitle aria-level={1} role="heading">
            {t("auth.loginTitle")}
          </CardTitle>
          <CardDescription>{t("auth.loginDescription")}</CardDescription>
        </CardHeader>
        <CardContent>
          <form
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
          </form>
          <Separator className="my-6" />
          <div className="flex flex-col gap-2">
            {session.data?.connectors.map((connector) => (
              <Button asChild key={connector.id} variant="outline">
                <a href={connector.authorizationUrl}>
                  {t("auth.continueWith", { name: connector.name })}
                </a>
              </Button>
            ))}
          </div>
        </CardContent>
        <CardFooter className="flex justify-between">
          <Button asChild variant="link">
            <Link to="/recovery">{t("auth.forgotPassword")}</Link>
          </Button>
          <Button disabled={mutation.isPending} form="login-form" type="submit">
            {t("auth.signIn")}
          </Button>
        </CardFooter>
      </Card>
    </main>
  );
}
