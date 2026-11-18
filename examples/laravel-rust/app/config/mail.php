<?php

return [
    'default' => env('MAIL_MAILER', 'smtp'),
    'mailers' => [
        'smtp' => [
            'transport' => 'smtp',
            'scheme' => env('MAIL_SCHEME'),
            'host' => env('MAIL_HOST', 'mailpit'),
            'port' => env('MAIL_PORT', 1025),
        ],
    ],
    'from' => [
        'address' => env('MAIL_FROM_ADDRESS', 'preview@laravel-rust.test'),
        'name' => env('MAIL_FROM_NAME', 'Laravel + Rust'),
    ],
];
