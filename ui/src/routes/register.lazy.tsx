import { Link, createLazyFileRoute } from '@tanstack/react-router';
import {
    Card,
    CardContent,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert';
import { CheckCircledIcon } from '@radix-ui/react-icons';

export const Route = createLazyFileRoute('/register')({
    component: Register,
})

function Register() {
    return (
        <div className="flex flex-col w-full h-full min-h-screen justify-center items-center">
            <Card className="max-w-lg w-full bg-slate-900 border-slate-600 p-2">
                <CardHeader>
                    <CardTitle className="text-center text-3xl">Register Account</CardTitle>
                </CardHeader>
                <CardContent className="gap-4 flex flex-col items-center justify-center space-y-2">
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="username">Username (SSO UI)</Label>
                        <Input type="text" id="username" placeholder="Username" />
                    </div>
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="password">Password (SSO UI)</Label>
                        <Input type="password" id="password" placeholder="Password" />
                    </div>
                    <div className="grid w-full max-w-sm items-center gap-1.5">
                        <Label className="text-md" htmlFor="fullName">Full Name</Label>
                        <Input type="text" id="fullName" placeholder="Full Name" />
                    </div>

                    <Alert variant="default" className="max-w-sm w-full border-yellow-300 text-yellow-300 bg-transparent">
                        <CheckCircledIcon className="h-4 w-4 !text-yellow-300" />
                        <AlertTitle>SSO UI Validated</AlertTitle>
                        <AlertDescription>
                            Our system will automatically check your data with SSO UI through a secure connection to confirm your identity
                        </AlertDescription>
                    </Alert>
                </CardContent>
                <CardFooter className="flex flex-col items-center justify-center space-y-4">
                    <Button size="lg" variant="default" className="w-2/3 text-foreground">
                        Register
                    </Button>
                </CardFooter>
            </Card>

        </div>
    )
}